# Service recordings (Mix Manager)

SanctuaryMix records each service: the audio mix and every console control
change, timestamped. Afterwards anyone on the team can open a past service,
listen to it, and watch (or carefully re-send) the fader and mute moves.

Recordings live on this computer only for now. Everything below is shaped so a
later sync to Supabase (Postgres rows + Storage objects, row-level security per
church) is an upload, not a migration.

## What gets recorded

| Part | Default | Format | Size |
| --- | --- | --- | --- |
| Main mix | On | Stereo WAV, 24-bit, console sample rate (48 kHz) | about 1.0 GB per hour |
| Multitrack | Off (opt-in) | One mono WAV per Dante input, 24-bit | about 0.52 GB per channel-hour, so 32 ch is about 16.6 GB per hour |
| Control moves | Always | Rows in SQLite + `events.jsonl` in the bundle | a few MB per service |

A typical Sunday (two 75-minute services) is about 2.6 GB with the default
stereo mix, or about 44 GB with 32-channel multitrack. That is why multitrack
is opt-in, shows its estimate before it starts, and has guards:

- Before starting, the app checks free disk space. Multitrack needs room for
  at least 3 hours; stereo needs room for at least 1 hour. Otherwise the app
  refuses to start that part and says why.
- While recording, if free space drops below 5 GB, multitrack stops on its own
  and the stereo mix and control log keep going. Below 1 GB everything stops
  cleanly and the files are finalized.
- Nothing is ever deleted automatically. The Mix Manager shows disk use and
  offers "Delete multitracks older than 30 days", which asks first.

The main mix comes from two Dante channels the user picks in Setup (on a dLive,
patch Main L/R to two Dante outputs). Multitrack records every input the audio
device offers.

WAV is portable (macOS, Windows, browsers, any DAW) and the writer refreshes the
header every few seconds, so a crash or power cut loses seconds, not the
service. Compressed copies (FLAC for archive, Opus for listening on a phone)
are a later step that fits the same bundle.

## Recording bundle (the unit that syncs)

Each recording is a folder named by its id under the app data directory
(`<app data>/recordings/<id>/`):

```
manifest.json     recording row as JSON (same field names as the table)
mix.wav           stereo main mix (if recorded)
tracks/in-01.wav  multitrack, one file per input (if recorded)
events.jsonl      every control change, one JSON object per line
```

A bundle folder copied into the recordings folder (from a backup, another
computer, or `scripts/make-sample-service.py`) is imported the next time the
app starts. A folder not named by its id is renamed to it.

The same relative paths become Supabase Storage object keys:
`org/<org_id>/recordings/<id>/mix.wav`, and so on.

## Data model (SQLite, `crates/recorder`)

Ids are UUID v7 strings (time-ordered, globally unique, so rows made offline
never collide when synced). Times are Unix epoch milliseconds (UTC). Every
synced table has `org_id`, `created_by`, `updated_at`, `deleted_at`
(soft delete, so deletions can sync) and `sync_state`.

`recordings`

| column | type | notes |
| --- | --- | --- |
| id | TEXT PK | UUID v7 |
| org_id | TEXT NULL | church; NULL while signed out ("local"), claimed on first sign-in |
| created_by | TEXT | auth user id (`local` today) |
| title | TEXT | "Sunday 9:00 AM" by default, editable |
| service_date | TEXT | local calendar date `YYYY-MM-DD`, groups services by Sunday |
| started_at / ended_at | INTEGER | epoch ms; ended_at NULL while recording |
| duration_ms | INTEGER | |
| status | TEXT | `recording`, `complete`, `interrupted` (app quit or crashed) |
| console_model | TEXT | e.g. `dlive`, `simulated` |
| sample_rate | INTEGER NULL | NULL when no audio |
| audio_mode | TEXT | `none`, `stereo`, `stereoMultitrack` |
| mix_channels | TEXT | JSON `[l, r]` 0-based device inputs, or NULL |
| track_count | INTEGER | |
| bytes_on_disk | INTEGER | |
| notes | TEXT | |
| sync_state | TEXT | `localOnly`, `pending`, `uploading`, `synced`, `failed` |
| remote_key | TEXT NULL | storage prefix once synced |
| created_at / updated_at / deleted_at | INTEGER | |

`recording_files`: id, recording_id, kind (`mix`, `track`, `events`,
`manifest`), channel (NULL for mix), rel_path, bytes, sha256 (filled when
finalized, used for upload integrity), sync_state.

`control_events`: recording_id, seq, t_ms (offset from recording start),
source, channel_kind, channel_index, kind (`fader`, `mute`, `name`,
`connected`, `disconnected`), value (JSON). Index on `(recording_id, t_ms)`.
The first rows of every recording are a full snapshot (source `snapshot`) of
every known channel, so seeking anywhere is "snapshot + events up to t".

Schema version lives in `PRAGMA user_version`; migrations are append-only.

## Control-event stream

`mix_core::ControlChange { at_ms, source, event }` is the one stream every
change goes through. Sources:

- `console`: someone moved it on the desk (echoes of our own sends are folded
  into the send that caused them).
- `operator`: a person changed it in SanctuaryMix.
- `assist`: the AI auto-mix changed it.
- `replay`: a replay sent it.
- `snapshot`: state captured at the start of a recording.

The Tauri shell owns a broadcast `ControlBus`. Commands publish `operator`,
the auto-mix loop publishes `assist`, the replay engine publishes `replay`, and
the console forwarder publishes `console` for anything that is not an echo of
a recent send. The recorder subscribes.

## Playback and replay

- **Listen.** Audio plays through the Rust engine to any output device,
  including Dante Virtual Soundcard outputs to put it on a PA. Play, pause,
  seek, and a position event about 10 times a second.
- **Watch (default).** The timeline shows the moves as lanes per channel, and
  a read-only mixer shows fader and mute state at the playhead. Nothing goes
  to the console.
- **Send moves to console (guarded).** Off by default and only for Admin and
  Engineer roles. It needs a confirmation that names the console, is blocked
  while a recording is running, can be limited to chosen channels, and stops
  at once if anyone touches the desk or the app (instant manual takeover). It
  captures the console state first, and "Restore" puts it back.

## Auth and cloud hooks

- Recordings are stamped with the current `auth::Session` (`user.id`,
  `active_org.id`). Signed out, `org_id` is NULL and `created_by` is `local`.
- `recorder::sync::RecordingSync` is the cloud boundary, like `AuthProvider`.
  Today it is `LocalOnly` (does nothing). A Supabase implementation will claim
  local rows into the signed-in church, upload the bundle to Storage, and
  upsert rows. The draft Postgres schema with row-level security is in
  `docs/supabase/recordings.sql` (not applied anywhere yet).
- Roles: everyone in a church can list and listen; Engineer and Admin can
  replay to a console and delete; Admin can change retention.

## On the website

Signed-in members can open a synced service on the website's account page
(`website/src/cloud`, `/account/service/?id=<id>`): listen, watch the fader and
mute timeline, and see every change. It is playback only, with no output to
choose and nothing sent to a console. Browsers stream a compressed copy,
`listen.mp3`, stored next to the bundle (`recording_files.kind = 'listen'`),
made when the bundle is uploaded.

Engineer and Admin can make a share link (`/share/?t=<token>`) that lets anyone
listen without an account, for 7 days, 30 days or until turned off, with or
without the moves. Draft schema and the public lookup: `docs/supabase/share_links.sql`.

Today the website reads a demo copy of the sample service from
`website/public/demo-cloud/`, laid out like the Storage keys, and keeps share
links in the browser. Nothing is uploaded yet.
