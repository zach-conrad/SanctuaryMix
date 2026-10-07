# Supabase

The website's and the desktop app's accounts run on the free Supabase project **SanctuaryMix** (`pfymsavkmnnxpbayyrsw`, us-east-2).

- `migrations/` holds the SQL applied to it, in order. `20261005190000_accounts.sql` adds churches (`organizations`), `memberships` (Admin, Engineer, Volunteer) and `create_church()`, with row-level security so people only see their own church. Clients can rename their church (Admins only) and nothing else; plans, trials and memberships are written only by database functions, and later by Stripe's webhook.
- `tests/accounts_rls.sql` checks those rules and rolls itself back. Paste it into the SQL editor; the RESULT line should say "ok" or "blocked" for every step.

The draft recordings and share-link schema stays in `docs/supabase/` until the app syncs recordings.

## One-time dashboard setup

These live in the Supabase and Google dashboards, not in code.

1. **Authentication → URL Configuration**: Site URL `https://sanctuarymix.vercel.app/account/`. Add redirect URLs `https://sanctuarymix.vercel.app/account/**` and, for local work, `http://localhost:5173/account/**`.
2. **Authentication → Sign In / Providers → Email**: keep Email on and **Confirm email** on. Set the minimum password length to 10 and require letters and digits.
3. **Google sign-in**: in Google Cloud Console, create an OAuth client (Web application) with the authorized redirect URI `https://pfymsavkmnnxpbayyrsw.supabase.co/auth/v1/callback`. Then in **Authentication → Sign In / Providers → Google**, turn it on and paste the client ID and secret.
4. **Before launch**: set up custom SMTP (Authentication → Emails → SMTP). Supabase's built-in sender allows only a couple of emails an hour and is for testing.

The website only ever holds the publishable key (`sb_publishable_…`). The secret and `service_role` keys never go in the website, the app, or this repo.

## The desktop app

The app signs in through the website, from the Rust core (`crates/auth/src/supabase.rs`), never from the webview:

1. Sign in in the app makes a secret, keeps it, and opens `https://sanctuarymix.vercel.app/account/?app_challenge=<SHA-256 of the secret>`.
2. The person signs in or creates an account on the website as usual (email, Google, confirm-email links all work; the challenge waits in the browser for 15 minutes).
3. The website calls `create_app_handoff(challenge)` (migration `20261005200000_app_handoff.sql`) for a one-time code and opens `sanctuarymix://auth/callback?code=<code>`.
4. The app posts the code and its secret to the `app-handoff` Edge Function (`supabase/functions/app-handoff`), which uses the code up (once, within five minutes), checks the secret against the challenge, and returns a separate session for the app. A code seen by anyone else is useless without the app's secret.

The refresh token and the last confirmed church and plan are kept in the macOS Keychain (Windows Credential Manager), so the app opens signed in without the network. It re-checks the plan at launch and every six hours, only between services, and keeps the last confirmed plan offline for 14 days.

The Edge Function is deployed with `verify_jwt` off (the app has no session yet) and reads `SUPABASE_URL` and `SUPABASE_SERVICE_ROLE_KEY`, which Supabase sets for every function. Redeploy it with `supabase functions deploy app-handoff --no-verify-jwt`.

Deep links only work from an installed build (`npm run app:build`), not `tauri dev` on macOS.

## Owner Admin page and church Teams

`/admin/` on the website lists every church and account and lets an owner extend a trial, change a plan, send a password reset or a confirmation/invite again, sign someone out everywhere, and turn sign-in off or on. Church Admins get a **Team** section on their Account page (invite by email, change role, remove). Migration `20261007180000_owner_admin.sql` adds:

- `private.platform_admins`: who is an owner. Filled by hand; no client can read or write it. Every `admin_*` function also requires a two-factor session (aal2), so the page asks owners to set up an authenticator app the first time.
- `private.admin_audit`: who changed what (owner actions and Team changes), shown under Recent changes.
- `admin_*` functions (owner reads and trial/plan/sign-out changes), `team_*` functions (church Admins), and the Essentials 3-login limit as a trigger on `memberships`.

Two Edge Functions, both deployed **with** JWT verification: `owner-admin` (password reset, resend email, disable/enable; owners with two-factor only) and `team-invite` (church Admins). They check the caller with the caller's own token before using the service role, and log to the audit table.

```
supabase functions deploy owner-admin
supabase functions deploy team-invite
```

Make someone an owner (SQL editor):

```sql
insert into private.platform_admins (user_id)
select id from auth.users where email = 'owner@example.com'
on conflict do nothing;
```

Emails these send (invites, owner-sent resets) land on `/account/` with the session in the link, where the person chooses a password. They use the redirect URL already allowed in step 1 above.
