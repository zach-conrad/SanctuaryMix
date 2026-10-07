-- Owner Admin page (website /admin/) and the church Team section.
-- Decisions A1-A7 in the owner admin research: an owner-only page on the
-- website, two-factor required, view + trial/plan changes + email/sign-in
-- actions, a Team section for church Admins, and an audit log of every change.
--
-- Who is an owner lives in private.platform_admins, filled by hand in the SQL
-- editor. No client can read or write it. Every owner function checks it AND
-- that this session passed two-factor (aal2), and refuses everyone else.
-- Actions that need the auth admin API (password reset, disable, invites) run
-- in the owner-admin and team-invite Edge Functions, which make the same
-- checks with the caller's own token before touching the service role.

-- ---- tables ----

create table private.platform_admins (
  user_id  uuid primary key references auth.users (id) on delete cascade,
  added_at timestamptz not null default now()
);
alter table private.platform_admins enable row level security;
revoke all on private.platform_admins from public, anon, authenticated;

-- Who changed what. No foreign keys on the targets so history outlives them.
create table private.admin_audit (
  id          bigint generated always as identity primary key,
  actor       uuid references auth.users (id) on delete set null,
  action      text not null,
  target_user uuid,
  target_org  uuid,
  detail      jsonb not null default '{}'::jsonb,
  created_at  timestamptz not null default now()
);
create index admin_audit_created_at on private.admin_audit (created_at desc);
create index admin_audit_actor on private.admin_audit (actor);
alter table private.admin_audit enable row level security;
revoke all on private.admin_audit from public, anon, authenticated;

-- ---- helpers (not exposed through the API) ----

create or replace function private.is_platform_admin()
returns boolean
language sql stable security definer set search_path = ''
as $$
  select exists (select 1 from private.platform_admins a where a.user_id = (select auth.uid()));
$$;

create or replace function private.has_mfa()
returns boolean
language sql stable set search_path = ''
as $$
  select coalesce((select auth.jwt()) ->> 'aal', '') = 'aal2';
$$;

create or replace function private.require_owner()
returns void
language plpgsql stable security definer set search_path = ''
as $$
begin
  if not private.is_platform_admin() then
    raise exception 'Not found' using errcode = '42501';
  end if;
  if not private.has_mfa() then
    raise exception 'Enter your two-factor code first' using errcode = '42501';
  end if;
end;
$$;

create or replace function private.audit(action text, target_user uuid, target_org uuid, detail jsonb)
returns void
language sql volatile security definer set search_path = ''
as $$
  insert into private.admin_audit (actor, action, target_user, target_org, detail)
  values ((select auth.uid()), action, target_user, target_org, coalesce(detail, '{}'::jsonb));
$$;

-- Team logins a plan allows (pricing: Essentials up to 3). Null means no limit.
create or replace function private.member_limit(plan text)
returns integer
language sql immutable set search_path = ''
as $$
  select case when plan = 'essentials' then 3 else null end;
$$;

revoke execute on all functions in schema private from public, anon;
grant execute on all functions in schema private to authenticated;
-- Writing the log is only for the functions above, never a caller.
revoke execute on function private.audit(text, uuid, uuid, jsonb) from authenticated;

-- Essentials' 3-login limit, enforced in the database (auth plan section 8).
create or replace function private.check_member_limit()
returns trigger
language plpgsql security definer set search_path = ''
as $$
declare
  cap integer;
begin
  -- One insert at a time per church, so two invites can't both squeeze in.
  perform pg_advisory_xact_lock(hashtextextended(new.org_id::text, 1));
  select private.member_limit(o.plan) into cap from public.organizations o where o.id = new.org_id;
  if cap is not null and (select count(*) from public.memberships m where m.org_id = new.org_id) >= cap then
    raise exception 'Essentials allows % team logins. Move to Pro for more.', cap using errcode = 'P0001';
  end if;
  return new;
end;
$$;

create trigger memberships_limit
  before insert on public.memberships
  for each row execute function private.check_member_limit();

-- ---- owner: is this person an owner, and did they pass two-factor? ----

-- Anyone signed in may ask; a non-owner just gets owner = false.
create or replace function public.owner_status()
returns jsonb
language sql stable security definer set search_path = ''
as $$
  select jsonb_build_object('owner', private.is_platform_admin(), 'mfa', private.has_mfa());
$$;

-- ---- owner: reads ----

create or replace function public.admin_overview()
returns jsonb
language plpgsql stable security definer set search_path = ''
as $$
begin
  perform private.require_owner();
  return jsonb_build_object(
    'churches', (select count(*) from public.organizations),
    'accounts', (select count(*) from auth.users),
    'trials_ending_week', (select count(*) from public.organizations o
                           where o.trial_ends_at >= now() and o.trial_ends_at < now() + interval '7 days'),
    'trials_ended', (select count(*) from public.organizations o where o.trial_ends_at < now()),
    'signups_week', (select count(*) from auth.users u where u.created_at >= now() - interval '7 days')
  );
end;
$$;

create or replace function public.admin_list_churches()
returns table (
  id uuid, name text, plan text, billing text, trial_ends_at timestamptz, created_at timestamptz,
  members bigint, admin_email text
)
language plpgsql stable security definer set search_path = ''
as $$
begin
  perform private.require_owner();
  return query
  select o.id, o.name, o.plan, o.billing, o.trial_ends_at, o.created_at,
         (select count(*) from public.memberships m where m.org_id = o.id),
         (select u.email::text from public.memberships m join auth.users u on u.id = m.user_id
          where m.org_id = o.id and m.role = 'admin' order by m.created_at limit 1)
  from public.organizations o
  order by o.created_at desc;
end;
$$;

create or replace function public.admin_list_accounts()
returns table (
  id uuid, email text, name text, provider text, confirmed boolean, invited boolean,
  created_at timestamptz, last_sign_in_at timestamptz, disabled boolean, owner boolean,
  org_id uuid, org_name text, role text
)
language plpgsql stable security definer set search_path = ''
as $$
begin
  perform private.require_owner();
  return query
  select u.id, u.email::text,
         coalesce(nullif(btrim(u.raw_user_meta_data ->> 'full_name'), ''), nullif(btrim(u.raw_user_meta_data ->> 'name'), '')),
         coalesce(u.raw_app_meta_data ->> 'provider', 'email'),
         u.email_confirmed_at is not null,
         u.invited_at is not null,
         u.created_at, u.last_sign_in_at,
         coalesce(u.banned_until > now(), false),
         exists (select 1 from private.platform_admins a where a.user_id = u.id),
         o.id, o.name, m.role
  from auth.users u
  left join lateral (
    select m.org_id, m.role from public.memberships m where m.user_id = u.id order by m.created_at limit 1
  ) m on true
  left join public.organizations o on o.id = m.org_id
  order by u.created_at desc;
end;
$$;

create or replace function public.admin_recent_changes(max_rows integer default 25)
returns table (created_at timestamptz, actor_email text, action text, target text, detail jsonb)
language plpgsql stable security definer set search_path = ''
as $$
begin
  perform private.require_owner();
  return query
  select a.created_at, actor.email::text, a.action,
         coalesce(tu.email::text, o.name, a.detail ->> 'email'),
         a.detail
  from private.admin_audit a
  left join auth.users actor on actor.id = a.actor
  left join auth.users tu on tu.id = a.target_user
  left join public.organizations o on o.id = a.target_org
  order by a.created_at desc
  limit least(greatest(coalesce(max_rows, 25), 1), 200);
end;
$$;

-- ---- owner: changes ----

create or replace function public.admin_set_trial(org uuid, ends_at timestamptz)
returns void
language plpgsql volatile security definer set search_path = ''
as $$
declare
  before_at timestamptz;
begin
  perform private.require_owner();
  if ends_at is null or ends_at > now() + interval '3 years' then
    raise exception 'Pick a trial end within three years' using errcode = '22023';
  end if;
  select o.trial_ends_at into before_at from public.organizations o where o.id = org for update;
  if not found then
    raise exception 'That church no longer exists' using errcode = 'P0002';
  end if;
  update public.organizations set trial_ends_at = ends_at where id = org;
  perform private.audit('church.trial', null, org, jsonb_build_object('before', before_at, 'after', ends_at));
end;
$$;

create or replace function public.admin_set_plan(org uuid, new_plan text, new_billing text)
returns void
language plpgsql volatile security definer set search_path = ''
as $$
declare
  before_plan text;
  before_billing text;
begin
  perform private.require_owner();
  if new_plan not in ('essentials', 'pro', 'campus') or new_billing not in ('monthly', 'yearly') then
    raise exception 'Pick a plan and billing period' using errcode = '22023';
  end if;
  select o.plan, o.billing into before_plan, before_billing from public.organizations o where o.id = org for update;
  if not found then
    raise exception 'That church no longer exists' using errcode = 'P0002';
  end if;
  update public.organizations set plan = new_plan, billing = new_billing where id = org;
  perform private.audit('church.plan', null, org, jsonb_build_object(
    'before', before_plan || ', ' || before_billing, 'after', new_plan || ', ' || new_billing));
end;
$$;

-- Ends every session (website and app). Access tokens already handed out
-- stop working when they expire, within the hour; the app keeps mixing
-- offline either way (booth rule).
create or replace function public.admin_sign_out_everywhere(target uuid)
returns integer
language plpgsql volatile security definer set search_path = ''
as $$
declare
  ended integer;
begin
  perform private.require_owner();
  delete from auth.sessions s where s.user_id = target;
  get diagnostics ended = row_count;
  perform private.audit('account.sign_out_everywhere', target, null, jsonb_build_object('sessions', ended));
  return ended;
end;
$$;

-- Edge Functions only (service role): log an action they took for a caller
-- they already checked.
create or replace function public.admin_record(actor uuid, action text, target_user uuid, target_org uuid, detail jsonb)
returns void
language sql volatile security definer set search_path = ''
as $$
  insert into private.admin_audit (actor, action, target_user, target_org, detail)
  values (actor, action, target_user, target_org, coalesce(detail, '{}'::jsonb));
$$;

-- ---- church Admins: the Team section ----

create or replace function private.require_org_admin(org uuid)
returns void
language plpgsql stable security definer set search_path = ''
as $$
begin
  if not private.is_org_admin(org) then
    raise exception 'Only your church''s Admin can do that' using errcode = '42501';
  end if;
end;
$$;
revoke execute on function private.require_org_admin(uuid) from public, anon;
grant execute on function private.require_org_admin(uuid) to authenticated;

create or replace function public.team_members(org uuid)
returns table (
  user_id uuid, email text, name text, role text, joined_at timestamptz,
  last_sign_in_at timestamptz, pending boolean
)
language plpgsql stable security definer set search_path = ''
as $$
begin
  perform private.require_org_admin(org);
  return query
  select m.user_id, u.email::text,
         coalesce(nullif(btrim(u.raw_user_meta_data ->> 'full_name'), ''), nullif(btrim(u.raw_user_meta_data ->> 'name'), '')),
         m.role, m.created_at, u.last_sign_in_at,
         u.email_confirmed_at is null
  from public.memberships m
  join auth.users u on u.id = m.user_id
  where m.org_id = org
  order by case m.role when 'admin' then 0 when 'engineer' then 1 else 2 end, m.created_at;
end;
$$;

create or replace function public.team_set_role(org uuid, member uuid, new_role text)
returns void
language plpgsql volatile security definer set search_path = ''
as $$
declare
  before_role text;
begin
  perform private.require_org_admin(org);
  if new_role not in ('admin', 'engineer', 'volunteer') then
    raise exception 'Pick Admin, Engineer or Volunteer' using errcode = '22023';
  end if;
  perform pg_advisory_xact_lock(hashtextextended(org::text, 1));
  select m.role into before_role from public.memberships m where m.org_id = org and m.user_id = member;
  if not found then
    raise exception 'That person isn''t on your team' using errcode = 'P0002';
  end if;
  if before_role = 'admin' and new_role <> 'admin'
     and (select count(*) from public.memberships m where m.org_id = org and m.role = 'admin') = 1 then
    raise exception 'Make someone else Admin first' using errcode = 'P0001';
  end if;
  update public.memberships set role = new_role where org_id = org and user_id = member;
  perform private.audit('team.role', member, org, jsonb_build_object('before', before_role, 'after', new_role));
end;
$$;

create or replace function public.team_remove_member(org uuid, member uuid)
returns void
language plpgsql volatile security definer set search_path = ''
as $$
declare
  before_role text;
begin
  perform private.require_org_admin(org);
  perform pg_advisory_xact_lock(hashtextextended(org::text, 1));
  select m.role into before_role from public.memberships m where m.org_id = org and m.user_id = member;
  if not found then
    return;
  end if;
  if before_role = 'admin'
     and (select count(*) from public.memberships m where m.org_id = org and m.role = 'admin') = 1 then
    raise exception 'Make someone else Admin first' using errcode = 'P0001';
  end if;
  delete from public.memberships where org_id = org and user_id = member;
  perform private.audit('team.remove', member, org, jsonb_build_object('role', before_role));
end;
$$;

-- Called by the team-invite Edge Function with the Admin's own token, before
-- it sends anything: refuses non-Admins and full Essentials teams.
create or replace function public.team_invite_check(org uuid)
returns void
language plpgsql stable security definer set search_path = ''
as $$
declare
  cap integer;
begin
  perform private.require_org_admin(org);
  select private.member_limit(o.plan) into cap from public.organizations o where o.id = org;
  if cap is not null and (select count(*) from public.memberships m where m.org_id = org) >= cap then
    raise exception 'Essentials allows % team logins. Move to Pro for more.', cap using errcode = 'P0001';
  end if;
end;
$$;

-- Edge Function only (service role): put an existing account on a church.
-- Returns 'added', 'added_unconfirmed' (they never finished signing up, so
-- the function re-sends their invite), or 'new' (no account yet; the function
-- invites them, then calls this again).
create or replace function public.team_add_member(org uuid, member_email text, new_role text, actor uuid)
returns text
language plpgsql volatile security definer set search_path = ''
as $$
declare
  target uuid;
  confirmed boolean;
begin
  if new_role not in ('admin', 'engineer', 'volunteer') then
    raise exception 'Pick Admin, Engineer or Volunteer' using errcode = '22023';
  end if;
  select u.id, u.email_confirmed_at is not null into target, confirmed
  from auth.users u where lower(u.email) = lower(btrim(member_email));
  if target is null then
    return 'new';
  end if;
  if exists (select 1 from public.memberships m where m.user_id = target and m.org_id = org) then
    raise exception 'They''re already on your team' using errcode = 'P0001';
  end if;
  if exists (select 1 from public.memberships m where m.user_id = target) then
    raise exception 'That person already belongs to another church' using errcode = 'P0001';
  end if;
  insert into public.memberships (org_id, user_id, role) values (org, target, new_role);
  insert into private.admin_audit (actor, action, target_user, target_org, detail)
  values (actor, 'team.add', target, org, jsonb_build_object('role', new_role));
  return case when confirmed then 'added' else 'added_unconfirmed' end;
end;
$$;

-- ---- who may call what ----

revoke execute on function
  public.owner_status(), public.admin_overview(), public.admin_list_churches(), public.admin_list_accounts(),
  public.admin_recent_changes(integer), public.admin_set_trial(uuid, timestamptz), public.admin_set_plan(uuid, text, text),
  public.admin_sign_out_everywhere(uuid), public.team_members(uuid), public.team_set_role(uuid, uuid, text),
  public.team_remove_member(uuid, uuid), public.team_invite_check(uuid)
  from public, anon;
grant execute on function
  public.owner_status(), public.admin_overview(), public.admin_list_churches(), public.admin_list_accounts(),
  public.admin_recent_changes(integer), public.admin_set_trial(uuid, timestamptz), public.admin_set_plan(uuid, text, text),
  public.admin_sign_out_everywhere(uuid), public.team_members(uuid), public.team_set_role(uuid, uuid, text),
  public.team_remove_member(uuid, uuid), public.team_invite_check(uuid)
  to authenticated;

revoke execute on function public.admin_record(uuid, text, uuid, uuid, jsonb), public.team_add_member(uuid, text, text, uuid)
  from public, anon, authenticated;
grant execute on function public.admin_record(uuid, text, uuid, uuid, jsonb), public.team_add_member(uuid, text, text, uuid)
  to service_role;
