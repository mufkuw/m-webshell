# Front-end Proxy Examples

m-webshell listens on `127.0.0.1:12479` and is not directly accessible from the network. You need a front-end proxy to forward requests to it. Below are ready-to-use configurations for nginx, Caddy, and Apache.

All examples assume:
- m-webshell is running on `127.0.0.1:12479`
- The TOTP-gated path is `/system/manage-`
- Invalid TOTP codes should show a generic 404 page (no hint that a terminal exists)

---

## nginx

```nginx
server {
    listen 80;
    listen [::]:80;
    server_name your-domain.com;

    # ... your existing locations (app, static assets, etc.) ...

    # m-webshell — TOTP-gated web terminal
    # Place this BEFORE any catch-all `location /` block
    location /system/manage- {
        proxy_pass http://127.0.0.1:12479;
        proxy_http_version 1.1;

        # WebSocket support
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        # Standard proxy headers
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # No timeout on terminal sessions
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;

        # Intercept 404s from m-webshell and show nginx's generic 404 page
        # instead of an empty response — no hint that a terminal exists
        proxy_intercept_errors on;
        error_page 404 = @m_webshell_404;
    }

    # Internal location for generic 404 response
    location @m_webshell_404 {
        return 404;
    }

    # Catch-all for your main application
    location / {
        # ... your app config ...
        proxy_pass http://127.0.0.1:8000;
    }
}
```

### With HTTPS (Let's Encrypt)

```nginx
server {
    listen 80;
    server_name your-domain.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name your-domain.com;

    ssl_certificate /etc/letsencrypt/live/your-domain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/your-domain.com/privkey.pem;

    # ... your existing locations ...

    location /system/manage- {
        proxy_pass http://127.0.0.1:12479;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
        proxy_intercept_errors on;
        error_page 404 = @m_webshell_404;
    }

    location @m_webshell_404 {
        return 404;
    }

    location / {
        # ... your app config ...
    }
}
```

---

## Caddy

```caddyfile
your-domain.com {
    # m-webshell — TOTP-gated web terminal
    # Use a named matcher for /system/manage-* paths
    @mwebshell path /system/manage-*
    handle @mwebshell {
        reverse_proxy 127.0.0.1:12479 {
            # WebSocket support (automatic in Caddy, but explicit for clarity)
            transport http {
                read_timeout 86400s
                write_timeout 86400s
            }
        }
    }

    # Your main application
    handle {
        reverse_proxy 127.0.0.1:8000
    }
}
```

Caddy automatically handles HTTPS with Let's Encrypt and WebSocket upgrades — no extra configuration needed. Invalid TOTP codes will pass through m-webshell's bare `404` response directly to the client.

---

## Apache 2

Enable the required modules:

```bash
a2enmod proxy proxy_http proxy_wstunnel rewrite ssl
systemctl restart apache2
```

### VirtualHost (HTTP)

```apache
<VirtualHost *:80>
    ServerName your-domain.com

    # m-webshell — TOTP-gated web terminal
    # Match /system/manage-<6-digits> paths
    <LocationMatch "^/system/manage-[0-9]{6}">
        ProxyPass "http://127.0.0.1:12479"
        ProxyPassReverse "http://127.0.0.1:12479"

        # WebSocket support
        RewriteEngine On
        RewriteCond %{HTTP:Upgrade} websocket [NC]
        RewriteCond %{HTTP:Connection} upgrade [NC]
        RewriteRule ^/?(.*) "ws://127.0.0.1:12479/$1" [P,L]

        # Proxy headers
        ProxyPreserveHost On
        RequestHeader set X-Real-IP "%{REMOTE_ADDR}s"
        RequestHeader set X-Forwarded-Proto "http"
    </LocationMatch>

    # Proxy timeout for long-running terminal sessions
    ProxyTimeout 86400

    # Your main application
    <Location "/">
        ProxyPass "http://127.0.0.1:8000/"
        ProxyPassReverse "http://127.0.0.1:8000/"
    </Location>
</VirtualHost>
```

### VirtualHost (HTTPS)

```apache
<VirtualHost *:443>
    ServerName your-domain.com

    SSLEngine on
    SSLCertificateFile /etc/letsencrypt/live/your-domain.com/fullchain.pem
    SSLCertificateKeyFile /etc/letsencrypt/live/your-domain.com/privkey.pem

    # m-webshell — TOTP-gated web terminal
    <LocationMatch "^/system/manage-[0-9]{6}">
        ProxyPass "http://127.0.0.1:12479"
        ProxyPassReverse "http://127.0.0.1:12479"

        RewriteEngine On
        RewriteCond %{HTTP:Upgrade} websocket [NC]
        RewriteCond %{HTTP:Connection} upgrade [NC]
        RewriteRule ^/?(.*) "ws://127.0.0.1:12479/$1" [P,L]

        ProxyPreserveHost On
        RequestHeader set X-Real-IP "%{REMOTE_ADDR}s"
        RequestHeader set X-Forwarded-Proto "https"
    </LocationMatch>

    ProxyTimeout 86400

    <Location "/">
        ProxyPass "http://127.0.0.1:8000/"
        ProxyPassReverse "http://127.0.0.1:8000/"
    </Location>
</VirtualHost>

# Redirect HTTP to HTTPS
<VirtualHost *:80>
    ServerName your-domain.com
    Redirect permanent / https://your-domain.com/
</VirtualHost>
```

---

## Cloudflare

If you're using Cloudflare in front of your proxy:

1. **DNS**: Set your domain to "Proxied" (orange cloud).
2. **SSL/TLS**: Set mode to "Full" or "Full (strict)".
3. **WebSocket**: Cloudflare supports WebSocket natively — no extra configuration needed.
4. **WAF / Rate Limiting**: You can add an additional Cloudflare rate limit rule for `/system/manage-*` paths, but m-webshell already rate-limits at the application level.

No special Cloudflare configuration is required — it will pass through HTTP and WebSocket traffic transparently to your front-end proxy.