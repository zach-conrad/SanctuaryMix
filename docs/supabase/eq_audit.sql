-- DRAFT: not applied to any Supabase project yet.
--
-- Every EQ change around a recorded service, synced from the app's EQ log
-- (AI EQ: soundcheck applies, live feedback cuts, speech tone moves, undos,
-- hand edits and hand-backs). Church members see all of it on the website,
-- names included; a share link shows it only when made with show_eq, and then
-- with roles but no names (see open_share in share_links.sql).
-- Builds on recordings.sql (is_org_member, is_org_engineer).

create table public.eq_audit (
  recording_id   uuid not null references public.recordings(id) on delete cascade,
  org_id         uuid not null references organizations(id) on delete cascade,
  seq            bigint not null,
  -- Position in the recording; null for soundcheck before recording started.
  t_ms           bigint,
  at             bigint not null,
  channel_kind   text not null,
  channel_index  integer not null,
  channel_name   text not null,
  action         text not null check (action in ('soundcheck', 'feedback', 'tone', 'undo', 'person', 'handBack')),
  -- Exact changes as shown, e.g. ["Low cut  off → 100 Hz", "320 Hz  0.0 → −2.0 dB"].
  changes        jsonb not null,
  reason         text,
  by_kind        text not null check (by_kind in ('ai', 'person')),
  by_where       text check (by_where in ('app', 'desk')),
  by_role        text check (by_role in ('admin', 'engineer', 'volunteer')),
  by_user        uuid references auth.users(id) on delete set null,
  -- Who tapped Apply on an AI EQ soundcheck proposal.
  applied_role   text check (applied_role in ('admin', 'engineer', 'volunteer')),
  applied_user   uuid references auth.users(id) on delete set null,
  primary key (recording_id, seq)
);

-- AI EQ's ideas for next week. Members only; never shared.
create table public.eq_ideas (
  id             uuid primary key default gen_random_uuid(),
  recording_id   uuid not null references public.recordings(id) on delete cascade,
  org_id         uuid not null references organizations(id) on delete cascade,
  channel_kind   text not null,
  channel_index  integer not null,
  channel_name   text not null,
  title          text not null,
  change         text not null,
  reason         text not null,
  state          text not null default 'waiting' check (state in ('waiting', 'kept', 'dismissed'))
);
create index eq_ideas_recording on public.eq_ideas (recording_id);

alter table public.eq_audit enable row level security;
alter table public.eq_ideas enable row level security;

-- Every member sees everything; the app (an Engineer or Admin's device) writes.
create policy "members read eq audit" on public.eq_audit
  for select using (public.is_org_member(org_id));
create policy "engineers write eq audit" on public.eq_audit
  for insert with check (public.is_org_engineer(org_id));
create policy "members read eq ideas" on public.eq_ideas
  for select using (public.is_org_member(org_id));
create policy "engineers write eq ideas" on public.eq_ideas
  for all using (public.is_org_engineer(org_id)) with check (public.is_org_engineer(org_id));

-- Names for the website come from the members' profiles via by_user and
-- applied_user; open_share never joins them.
