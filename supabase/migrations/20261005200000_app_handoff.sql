-- Signing the desktop app in through the website.
-- The table was applied to the "SanctuaryMix" Supabase project on 2026-10-05;
-- run the two functions below in the SQL editor if they are missing.
--
-- The app opens the website with a PKCE challenge (base64url SHA-256 of a
-- secret only the app holds). Once the person is signed in there, the website
-- calls create_app_handoff() and sends the browser to
-- sanctuarymix://auth/callback?code=<code>. The app posts the code and its
-- secret to the app-handoff Edge Function, which redeems the code (once,
-- within five minutes), checks the secret against the challenge, and returns
-- a new session for the app. A code seen by anyone else is useless without
-- the app's secret.

create table private.app_handoffs (
  code       text primary key,
  user_id    uuid not null references auth.users (id) on delete cascade,
  challenge  text not null check (challenge ~ '^[A-Za-z0-9_-]{43}$'),
  expires_at timestamptz not null default now() + interval '5 minutes'
);
create index app_handoffs_user_id on private.app_handoffs (user_id);
alter table private.app_handoffs enable row level security;
revoke all on private.app_handoffs from public, anon, authenticated;

-- Website, signed in: a one-time code for this person and the app's challenge.
create or replace function public.create_app_handoff(challenge text)
returns text
language plpgsql volatile security definer set search_path = ''
as $$
declare
  uid uuid := auth.uid();
  new_code text := replace(gen_random_uuid()::text || gen_random_uuid()::text, '-', '');
begin
  if uid is null then
    raise exception 'Sign in first' using errcode = '28000';
  end if;
  if challenge is null or challenge !~ '^[A-Za-z0-9_-]{43}$' then
    raise exception 'Start signing in from SanctuaryMix again' using errcode = '22023';
  end if;
  delete from private.app_handoffs h where h.expires_at < now() or h.user_id = uid;
  insert into private.app_handoffs (code, user_id, challenge) values (new_code, uid, challenge);
  return new_code;
end;
$$;

revoke execute on function public.create_app_handoff(text) from public, anon;
grant execute on function public.create_app_handoff(text) to authenticated;

-- Edge Function only (service role): use up a code, returning who it was for.
create or replace function public.redeem_app_handoff(handoff_code text)
returns table (user_id uuid, email text, challenge text)
language sql volatile security definer set search_path = ''
as $$
  with used as (
    delete from private.app_handoffs h
    where h.code = handoff_code
    returning h.user_id, h.challenge, h.expires_at
  )
  select used.user_id, u.email::text, used.challenge
  from used join auth.users u on u.id = used.user_id
  where used.expires_at > now();
$$;

revoke execute on function public.redeem_app_handoff(text) from public, anon, authenticated;
grant execute on function public.redeem_app_handoff(text) to service_role;
