-- DRAFT: not applied to any Supabase project yet.
--
-- Share links for recorded services (website: website/src/cloud). Anyone with
-- a link can listen without an account; nothing else about the church is
-- exposed. Builds on recordings.sql (is_org_member, is_org_engineer) and
-- eq_audit.sql.

create table public.share_links (
  id            uuid primary key default gen_random_uuid(),
  org_id        uuid not null references organizations(id) on delete cascade,
  recording_id  uuid not null references public.recordings(id) on delete cascade,
  -- 128-bit random, base64url, made by the client. Only a hash would be safer;
  -- revisit if links ever grant more than listening.
  token         text not null unique check (length(token) >= 22),
  created_by    uuid references auth.users(id) on delete set null,
  created_at    bigint not null,
  expires_at    bigint,
  revoked_at    bigint,
  show_moves    boolean not null default true,
  -- EQ changes are off unless the person sharing turns them on. Shared EQ
  -- shows roles, never names, and never the ideas for next week.
  show_eq       boolean not null default false
);
create index share_links_recording on public.share_links (recording_id);

alter table public.share_links enable row level security;

-- Members see their church's links; Engineer and Admin make and turn them off.
create policy "members read share links" on public.share_links
  for select using (public.is_org_member(org_id));
create policy "engineers insert share links" on public.share_links
  for insert with check (public.is_org_engineer(org_id) and created_by = auth.uid());
create policy "engineers revoke share links" on public.share_links
  for update using (public.is_org_engineer(org_id)) with check (public.is_org_engineer(org_id));

-- Public lookup for the share page. Security definer so anonymous visitors can
-- read exactly one recording (and its moves and EQ changes, if the link
-- allows) by token, and
-- nothing else. Returns no rows for unknown, expired or revoked tokens; the
-- page can't tell those apart, which is deliberate.
create or replace function public.open_share(p_token text)
returns table (
  recording   jsonb,
  org_name    text,
  events      jsonb,
  show_moves  boolean,
  eq          jsonb,
  expires_at  bigint,
  listen_key  text
)
language sql stable security definer set search_path = public
as $$
  select
    to_jsonb(r) - 'created_by' - 'notes',
    o.name,
    case when s.show_moves then (
      select coalesce(jsonb_agg(to_jsonb(e) order by e.seq), '[]'::jsonb)
      from control_events e where e.recording_id = r.id
    ) else '[]'::jsonb end,
    s.show_moves,
    case when s.show_eq then (
      select coalesce(jsonb_agg(jsonb_build_object(
        'seq', a.seq, 't_ms', a.t_ms, 'at', a.at,
        'channel_kind', a.channel_kind, 'channel_index', a.channel_index, 'channel_name', a.channel_name,
        'action', a.action, 'changes', a.changes, 'reason', a.reason,
        'by_kind', a.by_kind, 'by_where', a.by_where, 'by_role', a.by_role,
        'applied_role', a.applied_role
      ) order by a.seq), '[]'::jsonb)
      from eq_audit a where a.recording_id = r.id
    ) end,
    s.expires_at,
    (select r.remote_key || '/' || f.rel_path from recording_files f
      where f.recording_id = r.id and f.kind = 'listen' limit 1)
  from share_links s
  join recordings r on r.id = s.recording_id and r.deleted_at is null
  join organizations o on o.id = s.org_id
  where s.token = p_token
    and s.revoked_at is null
    and (s.expires_at is null or s.expires_at > (extract(epoch from now()) * 1000)::bigint);
$$;
grant execute on function public.open_share(text) to anon, authenticated;

-- Audio for a share page: the bucket stays private. An Edge Function
-- `share-audio` calls open_share(token) and, if it returns a row, answers with
-- a short-lived signed URL for listen_key (service role). Signed-in members get
-- signed URLs through the normal storage policies in recordings.sql.
--
-- recording_files.kind also gains 'listen': an MP3 made on upload so browsers
-- and phones can stream the mix (the WAV stays for download and the app):
--   alter table public.recording_files drop constraint recording_files_kind_check;
--   alter table public.recording_files add constraint recording_files_kind_check
--     check (kind in ('mix', 'track', 'events', 'manifest', 'listen'));
