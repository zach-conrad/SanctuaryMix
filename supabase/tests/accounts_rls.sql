-- Checks the accounts rules (migrations/20261005190000_accounts.sql) without
-- leaving anything behind: it always ends by raising an exception, which rolls
-- everything back. Run it in the SQL editor and read the RESULT line; every
-- step should say "ok" or "blocked".
do $$
declare res text := '';
begin
  insert into auth.users (id, instance_id, aud, role, email, email_confirmed_at, created_at, updated_at)
  values ('11111111-1111-1111-1111-111111111111', '00000000-0000-0000-0000-000000000000', 'authenticated', 'authenticated', 'a@test.invalid', now(), now(), now()),
         ('33333333-3333-3333-3333-333333333333', '00000000-0000-0000-0000-000000000000', 'authenticated', 'authenticated', 'c@test.invalid', null, now(), now());
  set local role authenticated;
  perform set_config('request.jwt.claims', '{"sub":"11111111-1111-1111-1111-111111111111","role":"authenticated"}', true);
  perform public.create_church('Church A');
  begin update public.organizations set name = 'Renamed'; res := res || 'rename ok; '; exception when others then res := res || 'rename FAILED ' || sqlerrm || '; '; end;
  begin update public.organizations set plan = 'campus'; res := res || 'plan change ALLOWED; '; exception when others then res := res || 'plan change blocked; '; end;
  begin perform public.create_church('Second'); res := res || 'second church ALLOWED; '; exception when others then res := res || 'second church blocked; '; end;
  begin insert into public.memberships select id, '33333333-3333-3333-3333-333333333333', 'admin' from public.organizations; res := res || 'member insert ALLOWED; '; exception when others then res := res || 'member insert blocked; '; end;
  perform set_config('request.jwt.claims', '{"sub":"33333333-3333-3333-3333-333333333333","role":"authenticated"}', true);
  begin perform public.create_church('Unconfirmed'); res := res || 'unconfirmed email ALLOWED; '; exception when others then res := res || 'unconfirmed email blocked; '; end;
  if (select count(*) from public.organizations) = 0 then res := res || 'other church hidden ok; '; else res := res || 'other church VISIBLE; '; end if;
  set local role anon;
  begin perform public.create_church('Anon'); res := res || 'signed-out create ALLOWED; '; exception when others then res := res || 'signed-out create blocked; '; end;
  raise exception 'RESULT: %', res;
end $$;
