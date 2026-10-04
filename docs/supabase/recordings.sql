-- DRAFT: not applied to any Supabase project yet.
--
-- Postgres mirror of the local recordings tables in crates/recorder
-- (see docs/RECORDINGS.md). Ids are the UUID v7 strings made on the device,
-- so rows created offline upsert without collisions. Times stay epoch ms
-- (bigint) to match the local rows exactly.
--
-- Assumes:
--   organizations(id uuid primary key, ...)
--   memberships(user_id uuid references auth.users, org_id uuid references organizations,
--               role text check (role in ('admin', 'engineer', 'volunteer')))

-- ---- helpers ----

create or replace function public.is_org_member(org uuid)
returns boolean
language sql stable security definer set search_path = public
as $$
  select exists (
    select 1 from memberships m where m.org_id = org and m.user_id = auth.uid()
  );
$$;

create or replace function public.is_org_engineer(org uuid)
returns boolean
language sql stable security definer set search_path = public
as $$
  select exists (
    select 1 from memberships m
    where m.org_id = org and m.user_id = auth.uid() and m.role in ('admin', 'engineer')
  );
$$;

-- ---- tables ----

create table public.recordings (
  id            uuid primary key,
  org_id        uuid not null references organizations(id) on delete cascade,
  created_by    uuid references auth.users(id) on delete set null,
  title         text not null,
  service_date  date not null,
  started_at    bigint not null,
  ended_at      bigint,
  duration_ms   bigint not null default 0,
  status        text not null check (status in ('recording', 'complete', 'interrupted')),
  console_model text not null,
  sample_rate   integer,
  audio_mode    text not null check (audio_mode in ('none', 'stereo', 'stereoMultitrack')),
  mix_channels  jsonb,
  track_count   integer not null default 0,
  bytes_on_disk bigint not null default 0,
  notes         text not null default '',
  -- Storage prefix: org/<org_id>/recordings/<id>
  remote_key    text,
  created_at    bigint not null,
  updated_at    bigint not null,
  deleted_at    bigint
);
create index recordings_org_started on public.recordings (org_id, started_at desc);

create table public.recording_files (
  id           uuid primary key,
  org_id       uuid not null references organizations(id) on delete cascade,
  recording_id uuid not null references public.recordings(id) on delete cascade,
  kind         text not null check (kind in ('mix', 'track', 'events', 'manifest')),
  channel      integer,
  rel_path     text not null,
  bytes        bigint not null default 0,
  -- Hex SHA-256 from the device; the upload is checked against it.
  sha256       text,
  updated_at   bigint not null,
  unique (recording_id, rel_path)
);

create table public.control_events (
  recording_id  uuid not null references public.recordings(id) on delete cascade,
  org_id        uuid not null references organizations(id) on delete cascade,
  seq           bigint not null,
  t_ms          bigint not null,
  source        text not null check (source in ('console', 'operator', 'assist', 'replay', 'snapshot')),
  channel_kind  text,
  channel_index integer,
  kind          text not null check (kind in ('fader', 'mute', 'name', 'connected', 'disconnected')),
  value         jsonb not null,
  primary key (recording_id, seq)
);
create index control_events_time on public.control_events (recording_id, t_ms);

create table public.recording_channels (
  recording_id  uuid not null references public.recordings(id) on delete cascade,
  org_id        uuid not null references organizations(id) on delete cascade,
  channel_kind  text not null,
  channel_index integer not null,
  name          text not null,
  primary key (recording_id, channel_kind, channel_index)
);

-- sync_state is device-only and is not mirrored here.

-- ---- row-level security ----
-- Everyone in a church can list and listen; Engineer and Admin can write and delete.

alter table public.recordings enable row level security;
alter table public.recording_files enable row level security;
alter table public.control_events enable row level security;
alter table public.recording_channels enable row level security;

create policy "members read recordings" on public.recordings
  for select using (public.is_org_member(org_id));
create policy "engineers insert recordings" on public.recordings
  for insert with check (public.is_org_engineer(org_id));
create policy "engineers update recordings" on public.recordings
  for update using (public.is_org_engineer(org_id)) with check (public.is_org_engineer(org_id));
create policy "engineers delete recordings" on public.recordings
  for delete using (public.is_org_engineer(org_id));

create policy "members read recording files" on public.recording_files
  for select using (public.is_org_member(org_id));
create policy "engineers insert recording files" on public.recording_files
  for insert with check (public.is_org_engineer(org_id));
create policy "engineers update recording files" on public.recording_files
  for update using (public.is_org_engineer(org_id)) with check (public.is_org_engineer(org_id));
create policy "engineers delete recording files" on public.recording_files
  for delete using (public.is_org_engineer(org_id));

create policy "members read control events" on public.control_events
  for select using (public.is_org_member(org_id));
create policy "engineers insert control events" on public.control_events
  for insert with check (public.is_org_engineer(org_id));
create policy "engineers update control events" on public.control_events
  for update using (public.is_org_engineer(org_id)) with check (public.is_org_engineer(org_id));
create policy "engineers delete control events" on public.control_events
  for delete using (public.is_org_engineer(org_id));

create policy "members read recording channels" on public.recording_channels
  for select using (public.is_org_member(org_id));
create policy "engineers insert recording channels" on public.recording_channels
  for insert with check (public.is_org_engineer(org_id));
create policy "engineers update recording channels" on public.recording_channels
  for update using (public.is_org_engineer(org_id)) with check (public.is_org_engineer(org_id));
create policy "engineers delete recording channels" on public.recording_channels
  for delete using (public.is_org_engineer(org_id));

-- ---- storage (notes, not yet written as SQL) ----
-- Private bucket `recordings`. Object keys: org/<org_id>/recordings/<recording_id>/<rel_path>
-- e.g. org/<org_id>/recordings/<id>/mix.wav, .../tracks/in-01.wav, .../events.jsonl.
-- Policies on storage.objects for bucket_id = 'recordings', taking the org from
-- (storage.foldername(name))[2]::uuid:
--   select: public.is_org_member(org)
--   insert / update / delete: public.is_org_engineer(org)
-- Large WAVs upload with resumable (TUS) uploads; the client compares the
-- finished object against recording_files.sha256 before marking it synced.
-- Listening on a phone uses short-lived signed URLs, never a public bucket.
