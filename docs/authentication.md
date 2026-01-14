# OIDC Authentication

nvim-web supports enterprise SSO using OpenID Connect (OIDC) with BeyondCorp-style access policies.

## Quick Start

### 1. Configure OIDC Provider

Add to your `config.toml`:

```toml
[auth]
enabled = true
issuer = "https://accounts.google.com"
client_id = "your-client-id.apps.googleusercontent.com"
client_secret = "your-client-secret"
redirect_uri = "https://your-domain.com/auth/callback"
scopes = ["openid", "email", "profile"]
```

### 2. Provider Presets

The following providers are supported with minimal configuration:

#### Google

```toml
[auth]
enabled = true
issuer = "https://accounts.google.com"
client_id = "xxx.apps.googleusercontent.com"
client_secret = "GOCSPX-xxx"
redirect_uri = "https://your-domain.com/auth/callback"
```

#### Okta

```toml
[auth]
enabled = true
issuer = "https://your-org.okta.com"
client_id = "0oaxxxxx"
client_secret = "xxxxx"
redirect_uri = "https://your-domain.com/auth/callback"
scopes = ["openid", "email", "profile", "groups"]
```

#### Azure AD

```toml
[auth]
enabled = true
issuer = "https://login.microsoftonline.com/{tenant-id}/v2.0"
client_id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
client_secret = "xxxxx"
redirect_uri = "https://your-domain.com/auth/callback"
```

## Authentication Flow

1. User visits `/auth/login`
2. Redirected to OIDC provider with PKCE challenge
3. User authenticates with provider
4. Redirected back to `/auth/callback` with authorization code
5. Server exchanges code for tokens
6. Session cookie set, user redirected to home

## BeyondCorp Access Policies

Restrict access based on user attributes:

```toml
[auth.policy]
# Only allow users from these email domains
allowed_domains = ["company.com", "partner.com"]

# Only allow users in these groups
allowed_groups = ["engineering", "devops"]

# Only allow connections from these IP ranges
allowed_ips = ["10.0.0.0/8", "192.168.1.0/24"]
```

## Session Configuration

```toml
[auth.session]
cookie_name = "nvim_web_session"  # Cookie name
max_age_secs = 86400              # 24 hours
secure = true                      # Require HTTPS
http_only = true                   # Prevent JS access
same_site = "Lax"                  # CSRF protection
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/auth/login` | GET | Initiate login flow |
| `/auth/callback` | GET | OAuth callback (internal) |
| `/auth/logout` | GET | Clear session |
| `/auth/me` | GET | Get current user info |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `NVIM_WEB_OIDC_ISSUER` | OIDC issuer URL |
| `NVIM_WEB_OIDC_CLIENT_ID` | OAuth client ID |
| `NVIM_WEB_OIDC_CLIENT_SECRET` | OAuth client secret |
| `NVIM_WEB_OIDC_REDIRECT_URI` | Callback URL |

## Security Considerations

1. **Always use HTTPS** - Session cookies require secure transport
2. **Validate redirect URIs** - Only configure exact callback URLs
3. **Use short session timeouts** - Balance security and convenience
4. **Enable group-based access** - Limit access to authorized teams
5. **Monitor auth logs** - Watch for failed authentication attempts
