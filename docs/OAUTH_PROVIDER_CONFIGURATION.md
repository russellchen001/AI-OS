# OAuth Provider configuration

AI-OS only shows account sign-in for Providers with a complete OAuth configuration. A Provider needs all four values below; partial configuration remains disabled and is shown as `Coming later`.

Replace `PROVIDER` with one of the registered prefixes: `OPENAI`, `ANTHROPIC`, `GOOGLE`, `GROK`, `DEEPSEEK`, `DOUBAO`, or `KIMI`.

```dotenv
VITE_AI_OS_PROVIDER_OAUTH_CLIENT_ID=
VITE_AI_OS_PROVIDER_OAUTH_AUTHORIZATION_URL=https://provider.example/authorize
VITE_AI_OS_PROVIDER_OAUTH_TOKEN_URL=https://provider.example/token
VITE_AI_OS_PROVIDER_OAUTH_SCOPES=scope-one scope-two
```

Use a public/native OAuth client that supports Authorization Code with PKCE and loopback redirects. Do not put a client secret in Vite environment variables; Vite values are part of the application bundle. Register loopback HTTP redirects with the Provider according to its native-app policy. AI-OS chooses an ephemeral `127.0.0.1` port for each attempt.

## End-to-end verification

1. Launch the Tauri application with one complete Provider configuration.
2. Confirm that only that operational Provider offers account sign-in; incomplete Providers remain `Coming later`.
3. Complete sign-in in the Provider's official browser page and return through the loopback callback.
4. Confirm that AI-OS tests the connection, discovers models, and saves the selected default model.
5. Open **Manage connection** and confirm the real expiry and refresh availability reported by the token response.
6. For a token with a refresh token, use **Refresh account access** and confirm the expiry advances.
7. Revoke or expire the refresh token and confirm AI-OS reports that sign-in must be renewed and offers **Sign in again**.

OAuth access and refresh tokens stay in macOS Keychain. Canonical Provider instances contain only the credential kind, Keychain account reference, expiry, and whether the token is actually refreshable.
