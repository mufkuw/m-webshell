use crate::cli::ProxyServer;

pub fn run_setup_proxy(server: &ProxyServer, domain: &str, port: u16) {
    let config = match server {
        ProxyServer::Nginx => nginx_config(domain, port),
        ProxyServer::Caddy => caddy_config(domain, port),
        ProxyServer::Apache => apache_config(domain, port),
    };

    println!();
    println!("  m-webshell proxy configuration for {} ({})", match server {
        ProxyServer::Nginx => "nginx",
        ProxyServer::Caddy => "Caddy",
        ProxyServer::Apache => "Apache",
    }, domain);
    println!();
    println!("{}\n", config);

    let file_path = match server {
        ProxyServer::Nginx => format!("/etc/nginx/sites-available/m-webshell-{}.conf", domain),
        ProxyServer::Caddy => "Caddyfile (append to existing)".to_string(),
        ProxyServer::Apache => format!("/etc/apache2/sites-available/m-webshell-{}.conf", domain),
    };

    println!("  Save this to: {}", file_path);
    println!();
    match server {
        ProxyServer::Nginx => {
            println!("  Then enable and reload:");
            println!("    sudo ln -s {} /etc/nginx/sites-enabled/", file_path);
            println!("    sudo nginx -t && sudo systemctl reload nginx");
        }
        ProxyServer::Caddy => {
            println!("  Then reload Caddy:");
            println!("    sudo systemctl reload caddy");
        }
        ProxyServer::Apache => {
            println!("  Then enable and reload:");
            println!("    sudo a2ensite m-webshell-{}.conf", domain);
            println!("    sudo apachectl configtest && sudo systemctl reload apache2");
        }
    }
    println!();
}

fn nginx_config(domain: &str, port: u16) -> String {
    format!(r#"server {{
    listen 80;
    listen [::]:80;
    server_name {domain};

    # m-webshell — TOTP-gated web terminal
    # Place this BEFORE any catch-all `location /` block
    location /system/manage- {{
        proxy_pass http://127.0.0.1:{port};
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
    }}

    # Internal location for generic 404 response
    location @m_webshell_404 {{
        return 404;
    }}

    # Catch-all for your main application
    location / {{
        # ... your app config ...
        proxy_pass http://127.0.0.1:8000;
    }}
}}"#)
}

fn caddy_config(domain: &str, port: u16) -> String {
    format!(r#"{domain} {{
    # m-webshell — TOTP-gated web terminal
    @mwebshell path /system/manage-*
    handle @mwebshell {{
        reverse_proxy 127.0.0.1:{port} {{
            transport http {{
                read_timeout 86400s
                write_timeout 86400s
            }}
        }}
    }}

    # Your main application
    handle {{
        reverse_proxy 127.0.0.1:8000
    }}
}}"#)
}

fn apache_config(domain: &str, port: u16) -> String {
    format!(r#"<VirtualHost *:80>
    ServerName {domain}

    # m-webshell — TOTP-gated web terminal
    <LocationMatch "^/system/manage-[0-9]{{6}}">
        ProxyPass "http://127.0.0.1:{port}"
        ProxyPassReverse "http://127.0.0.1:{port}"

        # WebSocket support
        RewriteEngine On
        RewriteCond %{{HTTP:Upgrade}} websocket [NC]
        RewriteCond %{{HTTP:Connection}} upgrade [NC]
        RewriteRule ^/?(.*) "ws://127.0.0.1:{port}/$1" [P,L]

        ProxyPreserveHost On
        RequestHeader set X-Real-IP "%{{REMOTE_ADDR}}s"
        RequestHeader set X-Forwarded-Proto "http"
    </LocationMatch>

    # Proxy timeout for long-running terminal sessions
    ProxyTimeout 86400

    # Your main application
    <Location "/">
        ProxyPass "http://127.0.0.1:8000/"
        ProxyPassReverse "http://127.0.0.1:8000/"
    </Location>
</VirtualHost>"#)
}