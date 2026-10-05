# Supabase

The website's and the desktop app's accounts run on the free Supabase project **SanctuaryMix** (`pfymsavkmnnxpbayyrsw`, us-east-2).

- `migrations/` holds the SQL applied to it, in order. `20261005190000_accounts.sql` adds churches (`organizations`), `memberships` (Admin, Engineer, Volunteer) and `create_church()`, with row-level security so people only see their own church. Clients can rename their church (Admins only) and nothing else; plans, trials and memberships are written only by database functions, and later by Stripe's webhook.
- `tests/accounts_rls.sql` checks those rules and rolls itself back. Paste it into the SQL editor; the RESULT line should say "ok" or "blocked" for every step.

The draft recordings and share-link schema stays in `docs/supabase/` until the app syncs recordings.

## One-time dashboard setup

These live in the Supabase and Google dashboards, not in code.

1. **Authentication → URL Configuration**: Site URL `https://sanctuarymix.vercel.app/account/`. Add redirect URLs `https://sanctuarymix.vercel.app/account/**`, `sanctuarymix://auth/callback` (the desktop app's Continue with Google) and, for local work, `http://localhost:5173/account/**`.
2. **Authentication → Sign In / Providers → Email**: keep Email on and **Confirm email** on. Set the minimum password length to 10 and require letters and digits.
3. **Google sign-in**: in Google Cloud Console, create an OAuth client (Web application) with the authorized redirect URI `https://pfymsavkmnnxpbayyrsw.supabase.co/auth/v1/callback`. Then in **Authentication → Sign In / Providers → Google**, turn it on and paste the client ID and secret.
4. **Before launch**: set up custom SMTP (Authentication → Emails → SMTP). Supabase's built-in sender allows only a couple of emails an hour and is for testing.

The website only ever holds the publishable key (`sb_publishable_…`). The secret and `service_role` keys never go in the website, the app, or this repo.

## The desktop app

The app signs in from the Rust core (`crates/auth/src/supabase.rs`), never from the webview: email and password go straight to Supabase Auth, and Continue with Google opens the browser with PKCE and comes back through `sanctuarymix://auth/callback`. The refresh token and the last confirmed church and plan are kept in the macOS Keychain (Windows Credential Manager), so the app opens signed in without the network. It re-checks the plan at launch and every six hours, only between services, and keeps the last confirmed plan offline for 14 days. Accounts are created, and passwords reset, on the website.

Deep links only work from an installed build (`npm run app:build`), not `tauri dev` on macOS.
