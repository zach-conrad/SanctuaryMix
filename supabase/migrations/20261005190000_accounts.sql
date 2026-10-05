-- Accounts: churches and who belongs to them (auth doc sections 2 and 8).
-- Applied to the "SanctuaryMix" Supabase project on 2026-10-05.
--
-- The first person to sign up creates a church through create_church() and
-- becomes its Admin. Invites (Engineer, Volunteer) and Stripe come later; until
-- then the church row carries the plan picked at sign-up and the trial window.
-- Clients can rename their church (Admins only) and nothing else: plan, trial
-- and memberships are written only by security definer functions.

create schema if not exists private;

-- ---- tables ----

create table public.organizations (
  id            uuid primary key default gen_random_uuid(),
  name          text not null check (char_length(btrim(name)) between 1 and 120),
  created_by    uuid references auth.users (id) on delete set null,
  created_at    timestamptz not null default now(),
  -- Until billing lands: the plan picked on the pricing page, and the trial.
  plan          text not null default 'pro' check (plan in ('essentials', 'pro', 'campus')),
  billing       text not null default 'yearly' check (billing in ('monthly', 'yearly')),
  trial_ends_at timestamptz not null default now() + interval '30 days'
);

create table public.memberships (
  org_id     uuid not null references public.organizations (id) on delete cascade,
  user_id    uuid not null references auth.users (id) on delete cascade,
  role       text not null check (role in ('admin', 'engineer', 'volunteer')),
  created_at timestamptz not null default now(),
  primary key (org_id, user_id)
);
create index memberships_user_id on public.memberships (user_id);
create index organizations_created_by on public.organizations (created_by);

alter table public.organizations enable row level security;
alter table public.memberships enable row level security;

-- ---- helpers (not exposed through the API) ----

create or replace function private.is_org_member(org uuid)
returns boolean
language sql stable security definer set search_path = ''
as $$
  select exists (
    select 1 from public.memberships m
    where m.org_id = org and m.user_id = (select auth.uid())
  );
$$;

create or replace function private.is_org_admin(org uuid)
returns boolean
language sql stable security definer set search_path = ''
as $$
  select exists (
    select 1 from public.memberships m
    where m.org_id = org and m.user_id = (select auth.uid()) and m.role = 'admin'
  );
$$;

revoke all on schema private from public;
grant usage on schema private to authenticated;
revoke execute on all functions in schema private from public, anon;
grant execute on all functions in schema private to authenticated;

-- ---- policies ----

create policy "Members read their church"
  on public.organizations for select to authenticated
  using (private.is_org_member(id));

create policy "Admins rename their church"
  on public.organizations for update to authenticated
  using (private.is_org_admin(id))
  with check (private.is_org_admin(id));

create policy "Members read their church's members"
  on public.memberships for select to authenticated
  using (private.is_org_member(org_id));

-- Belt and braces under RLS: clients may only change a church's name.
revoke insert, update, delete, truncate on public.organizations from anon, authenticated;
grant update (name) on public.organizations to authenticated;
revoke insert, update, delete, truncate on public.memberships from anon, authenticated;
revoke all on public.organizations, public.memberships from anon;

-- ---- create a church ----

-- Called once by a newly signed-up person. Makes the church and an Admin
-- membership for them. Refuses unconfirmed emails and people who already
-- belong to a church.
create or replace function public.create_church(church_name text, plan text default 'pro', billing text default 'yearly')
returns uuid
language plpgsql volatile security definer set search_path = ''
as $$
declare
  uid uuid := auth.uid();
  org uuid;
begin
  if uid is null then
    raise exception 'Sign in first' using errcode = '28000';
  end if;
  -- One church per person at a time, even if two tabs race.
  perform pg_advisory_xact_lock(hashtextextended(uid::text, 0));
  if not exists (select 1 from auth.users u where u.id = uid and u.email_confirmed_at is not null) then
    raise exception 'Confirm your email first' using errcode = '28000';
  end if;
  if exists (select 1 from public.memberships m where m.user_id = uid) then
    raise exception 'You already belong to a church' using errcode = 'P0001';
  end if;
  if char_length(btrim(coalesce(church_name, ''))) not between 1 and 120 then
    raise exception 'Enter your church name' using errcode = '22023';
  end if;

  insert into public.organizations (name, created_by, plan, billing)
  values (
    btrim(church_name),
    uid,
    case when plan in ('essentials', 'pro', 'campus') then plan else 'pro' end,
    case when billing in ('monthly', 'yearly') then billing else 'yearly' end
  )
  returning id into org;

  insert into public.memberships (org_id, user_id, role) values (org, uid, 'admin');
  return org;
end;
$$;

revoke execute on function public.create_church(text, text, text) from public, anon;
grant execute on function public.create_church(text, text, text) to authenticated;
