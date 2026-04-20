# OpenVox WebUI - Troubleshooting Guide

Common issues and their solutions.

## Table of Contents

- [Authentication Issues](#authentication-issues)
- [SAML SSO Issues](#saml-sso-issues)
- [PuppetDB Connection Issues](#puppetdb-connection-issues)
- [Node Classification Issues](#node-classification-issues)
- [Certificate Management Issues](#certificate-management-issues)
- [Performance Issues](#performance-issues)
- [Alert Issues](#alert-issues)
- [UI Issues](#ui-issues)
- [Database Issues](#database-issues)
- [General Debugging](#general-debugging)

---

## Authentication Issues

### Cannot Login with Local Credentials

**Symptoms:**
- "Invalid username or password" error
- Account locked message

**Solutions:**

1. **Verify credentials are correct:**
   ```bash
   # Check if user exists
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "SELECT username, auth_provider FROM users WHERE username='your-username';"
   ```

2. **Check if account is locked:**
   ```bash
   # Check lock status
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "SELECT username, locked_until FROM users WHERE username='your-username';"

   # Unlock account
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "UPDATE users SET locked_until=NULL, failed_login_attempts=0 WHERE username='your-username';"
   ```

3. **Reset password (admin):**
   ```bash
   # Use bcrypt to hash new password
   python3 -c "import bcrypt; print(bcrypt.hashpw(b'newpassword', bcrypt.gensalt()).decode())"

   # Update password in database
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "UPDATE users SET password_hash='<bcrypt-hash>' WHERE username='your-username';"
   ```

4. **Check authentication provider:**
   - If user has `auth_provider='saml'`, they can only use SSO
   - Change to `'both'` or `'local'` to enable password login

### JWT Token Expired

**Symptoms:**
- "Token expired" error
- Redirected to login unexpectedly

**Solutions:**

1. **Check token expiry settings:**
   ```yaml
   # config.yaml
   auth:
     token_expiry_hours: 24  # Increase if needed
     refresh_token_expiry_days: 7
   ```

2. **Clear browser cache/cookies:**
   - Browser developer tools → Application → Storage
   - Clear localStorage and cookies for your domain

3. **Verify JWT secret hasn't changed:**
   - Changing `jwt_secret` in config invalidates all tokens
   - Users must re-login after secret change

### Session Timeout Too Aggressive

**Symptoms:**
- Logged out frequently during active use

**Solutions:**

```yaml
# config.yaml
auth:
  session_timeout: 3600  # Increase (seconds)
  token_expiry_hours: 24  # Increase access token lifetime
  refresh_token_expiry_days: 7  # Increase refresh token lifetime
```

---

## SAML SSO Issues

### Error: "No authentication token received"

**Symptoms:**
- SAML login redirects to callback but shows error
- No tokens in URL

**Cause:**
- IdP POST to `/saml-callback` instead of `/api/v1/auth/saml/acs`

**Solution:**

1. **Verify ACS URL in IdP configuration:**
   ```
   Correct: https://your-server.example.com/api/v1/auth/saml/acs
   Wrong: https://your-server.example.com/saml-callback
   ```

2. **Check SP metadata in IdP:**
   ```bash
   curl https://your-server.example.com/api/v1/auth/saml/metadata
   # Verify AssertionConsumerService URL is correct
   ```

3. **Update OpenVox to v0.15.1+** (includes HTTP 303 fix)

### Error: "User not found" or "Access Denied"

**Symptoms:**
- SAML authentication succeeds at IdP
- Error at OpenVox: "Your account has not been provisioned"

**Solutions:**

1. **Create user in OpenVox:**
   ```bash
   # User MUST exist before SSO login
   Navigate to: Users → Create User
   Username: <must match IdP username attribute>
   Auth Provider: saml or both
   ```

2. **Check username mapping:**
   ```yaml
   # config.yaml
   saml:
     user_mapping:
       username_attribute: "sAMAccountName"  # or "email", "username"
       email_attribute: "mail"
   ```

3. **Verify user has correct auth_provider:**
   ```bash
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "UPDATE users SET auth_provider='saml' WHERE username='your-username';"
   ```

4. **Check IdP sends expected attributes:**
   - Enable debug logging
   - Check logs for SAML assertion attributes
   - Verify IdP sends the username_attribute you configured

### Error: "SAML response validation failed"

**Symptoms:**
- Error after IdP authentication
- "Invalid signature" or "Expired assertion"

**Solutions:**

1. **Verify IdP metadata is current:**
   ```bash
   # Get fresh metadata from IdP
   curl https://idp.example.com/metadata.xml > /etc/openvox-webui/saml-metadata.xml
   sudo systemctl restart openvox-webui
   ```

2. **Check certificate validity:**
   ```bash
   # Extract certificate from metadata
   grep -A 10 "X509Certificate" /etc/openvox-webui/saml-metadata.xml

   # Check expiration
   echo "<certificate>" | base64 -d | openssl x509 -text -noout | grep "Not After"
   ```

3. **Verify time synchronization:**
   ```bash
   # SAML assertions are time-sensitive
   timedatectl status
   # Ensure NTP is enabled and synced
   ```

4. **Check audience restriction:**
   ```yaml
   # config.yaml - entity_id must match IdP's expected audience
   saml:
     sp:
       entity_id: "https://your-server.example.com/saml"
   ```

### Error: HTTP 405 Method Not Allowed

**Symptoms:**
- 405 error when IdP redirects back to OpenVox
- Browser shows "Method Not Allowed"

**Cause:**
- Using OpenVox < v0.15.1
- HTTP 307 redirect preserves POST method

**Solution:**

Upgrade to OpenVox v0.15.1 or later (uses HTTP 303 redirect)

### IdP-Initiated Login Not Working

**Symptoms:**
- SP-initiated login works (clicking "Login with SSO")
- IdP-initiated login fails (starting from IdP portal)

**Solution:**

1. **Enable IdP-initiated login:**
   ```yaml
   # config.yaml
   saml:
     user_mapping:
       allow_idp_initiated: true  # Default: false
   ```

2. **Note:** IdP-initiated login has security trade-offs
   - Vulnerable to login CSRF
   - Consider requiring SP-initiated only

---

## PuppetDB Connection Issues

### Error: "Cannot connect to PuppetDB"

**Symptoms:**
- Dashboard shows no nodes
- "Connection refused" or timeout errors

**Solutions:**

1. **Verify PuppetDB is running:**
   ```bash
   systemctl status puppetdb
   curl -k https://puppetdb.example.com:8081/pdb/meta/v1/version
   ```

2. **Check network connectivity:**
   ```bash
   telnet puppetdb.example.com 8081
   # Should connect successfully
   ```

3. **Verify SSL certificates:**
   ```bash
   # Test with curl
   curl --cert /path/to/client-cert.pem \
        --key /path/to/client-key.pem \
        --cacert /path/to/ca.pem \
        https://puppetdb.example.com:8081/pdb/meta/v1/version
   ```

4. **Check OpenVox configuration:**
   ```yaml
   # config.yaml
   puppetdb:
     url: "https://puppetdb.example.com:8081"
     ssl_verify: true
     ssl_cert: "/etc/puppetlabs/puppet/ssl/certs/openvox.pem"
     ssl_key: "/etc/puppetlabs/puppet/ssl/private_keys/openvox.pem"
     ssl_ca: "/etc/puppetlabs/puppet/ssl/certs/ca.pem"
     timeout_secs: 30
   ```

5. **Check file permissions:**
   ```bash
   ls -l /etc/puppetlabs/puppet/ssl/private_keys/openvox.pem
   # Should be readable by openvox-webui process user
   chmod 640 /etc/puppetlabs/puppet/ssl/private_keys/openvox.pem
   chown openvox-webui:openvox-webui /etc/puppetlabs/puppet/ssl/private_keys/openvox.pem
   ```

### Error: "SSL certificate verification failed"

**Solutions:**

1. **Temporarily disable SSL verification to test:**
   ```yaml
   # config.yaml (testing only!)
   puppetdb:
     ssl_verify: false
   ```

2. **Fix certificate issues:**
   ```bash
   # Verify certificate matches hostname
   openssl x509 -in /path/to/cert.pem -text -noout | grep -A1 "Subject Alternative Name"

   # Check certificate isn't expired
   openssl x509 -in /path/to/cert.pem -text -noout | grep "Not After"
   ```

3. **Regenerate certificates if needed:**
   ```bash
   # On Puppet server
   puppetserver ca generate --certname openvox-webui.example.com
   ```

### Slow PuppetDB Queries

**Symptoms:**
- Pages take long to load
- Timeout errors

**Solutions:**

1. **Enable caching:**
   ```yaml
   # config.yaml
   cache:
     ttl: 300  # 5 minutes
     max_entries: 1000
   ```

2. **Increase timeout:**
   ```yaml
   # config.yaml
   puppetdb:
     timeout_secs: 60  # Increase from default
   ```

3. **Optimize PuppetDB:**
   ```bash
   # Check PuppetDB performance
   # /etc/puppetlabs/puppetdb/conf.d/database.ini
   [database]
   gc-interval = 60  # Run garbage collection more frequently
   ```

4. **Use environment filters:**
   - Filter nodes by environment in UI
   - Reduces query size

---

## Node Classification Issues

### Nodes Not Matching Group Rules

**Symptoms:**
- Created group with rules
- Expected nodes don't appear in group

**Solutions:**

1. **Verify node has the fact:**
   ```bash
   # In UI
   Navigate to: Nodes → Select Node → Facts Tab
   Search for the fact path used in your rule
   ```

2. **Check fact path syntax:**
   ```yaml
   # Correct:
   fact_path: os.family

   # Wrong:
   fact_path: facts.os.family  # 'facts.' prefix not needed
   ```

3. **Verify fact value matches:**
   ```yaml
   # Case-sensitive!
   fact_path: os.family
   operator: =
   value: RedHat  # Must match exactly (not "redhat" or "Redhat")
   ```

4. **Check rule match type:**
   - `all`: Node must match ALL rules
   - `any`: Node must match ANY rule
   - Verify you're using the right one

5. **Test with single simple rule:**
   ```yaml
   # Start simple, add complexity later
   fact_path: kernel
   operator: =
   value: Linux
   ```

### Classification Not Applied to Puppet

**Symptoms:**
- Group shows nodes as members
- Puppet doesn't receive classes from the group

**Cause:**
- OpenVox classification is separate from Puppet's ENC
- Need to configure Puppet to use OpenVox as ENC

**Solution:**

Configure Puppet to query OpenVox for classification:

```ini
# /etc/puppetlabs/puppet/puppet.conf
[master]
node_terminus = exec
external_nodes = /usr/local/bin/openvox-enc

[agent]
server = puppet.example.com
```

Create ENC script:
```bash
#!/bin/bash
# /usr/local/bin/openvox-enc

CERTNAME=$1
API_URL="https://openvox.example.com/api/v1"
API_KEY="your-api-key"

curl -s -H "X-API-Key: $API_KEY" \
  "$API_URL/nodes/$CERTNAME/classification" \
  | jq -r '.classes, .parameters, .environment'
```

### Variables Not Available in Facter Templates

**Symptoms:**
- Variables defined in group
- Not rendered in external facts

**Solutions:**

1. **Verify node is in the group:**
   ```bash
   Navigate to: Nodes → Select Node → Classification Tab
   # Should list the group with variables
   ```

2. **Check template syntax:**
   ```yaml
   # Correct:
   my_var: {{variables.datacenter}}

   # Wrong:
   my_var: {{datacenter}}  # Missing 'variables.' prefix
   ```

3. **Export facts and check output:**
   ```bash
   curl -H "X-API-Key: $API_KEY" \
     "https://openvox.example.com/api/v1/facter/export/node01?template=my-template"
   # Verify variables are rendered
   ```

---

## Certificate Management Issues

### CSRs Not Appearing

**Symptoms:**
- Node generated CSR
- Not visible in OpenVox WebUI

**Solutions:**

1. **Verify Puppet CA connectivity:**
   ```bash
   # Test CA API
   curl --cert /path/to/cert.pem \
        --key /path/to/key.pem \
        --cacert /path/to/ca.pem \
        https://puppet.example.com:8140/puppet-ca/v1/certificate_requests
   ```

2. **Check Puppet CA configuration:**
   ```yaml
   # config.yaml
   puppet_ca:
     url: "https://puppet.example.com:8140"
     ssl_cert: "/etc/puppetlabs/puppet/ssl/certs/openvox.pem"
     ssl_key: "/etc/puppetlabs/puppet/ssl/private_keys/openvox.pem"
     ssl_ca: "/etc/puppetlabs/puppet/ssl/certs/ca.pem"
   ```

3. **Verify on Puppet server:**
   ```bash
   sudo puppetserver ca list
   # Should show pending requests
   ```

### Cannot Sign Certificates

**Symptoms:**
- Click "Sign" button
- Error: "Failed to sign certificate"

**Solutions:**

1. **Check API permissions:**
   ```bash
   # Puppet server auth.conf or certificate_authority settings
   # OpenVox needs permission to sign certificates
   ```

2. **Verify SSL client certificate:**
   ```bash
   # OpenVox's client cert must be authorized
   openssl x509 -in /path/to/client-cert.pem -text -noout
   ```

3. **Sign manually and verify:**
   ```bash
   # On Puppet server
   sudo puppetserver ca sign --certname node01.example.com

   # Verify it appears in OpenVox
   ```

### CA Certificate Expiration Warnings

**Symptoms:**
- Dashboard shows "CA certificate expires in X days"

**Solution:**

Renew CA certificate before expiration:

```bash
# Backup current CA
sudo cp -r /etc/puppetlabs/puppet/ssl/ca /etc/puppetlabs/puppet/ssl/ca.backup

# On Puppet server (Puppet 6+)
sudo puppetserver ca renew --ca

# Distribute new CA cert to all agents
# This is a major operation - plan carefully
```

---

## Performance Issues

### Dashboard Loading Slowly

**Solutions:**

1. **Enable caching:**
   ```yaml
   # config.yaml
   cache:
     ttl: 300
     max_entries: 1000
   ```

2. **Reduce auto-refresh frequency:**
   ```yaml
   # config.yaml
   dashboard:
     refresh_interval: 60  # Increase from 30
   ```

3. **Use environment filters:**
   - Filter dashboard to specific environments
   - Reduces data fetched from PuppetDB

4. **Optimize PuppetDB queries:**
   - Add indexes to PuppetDB database
   - Tune PostgreSQL performance

### High Memory Usage

**Solutions:**

1. **Reduce cache size:**
   ```yaml
   # config.yaml
   cache:
     max_entries: 500  # Reduce from 1000
   ```

2. **Limit database connections:**
   ```yaml
   # config.yaml
   database:
     max_connections: 5  # Reduce from 10
   ```

3. **Monitor and restart if needed:**
   ```bash
   # Check memory usage
   systemctl status openvox-webui

   # Restart service
   sudo systemctl restart openvox-webui
   ```

### Database Growing Too Large

**Solutions:**

1. **Vacuum database:**
   ```bash
   sqlite3 /var/lib/openvox-webui/openvox.db "VACUUM;"
   ```

2. **Archive old audit logs:**
   ```bash
   # Export old logs
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "SELECT * FROM audit_logs WHERE created_at < date('now', '-90 days');" \
     > audit_logs_archive.csv

   # Delete old logs
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "DELETE FROM audit_logs WHERE created_at < date('now', '-90 days');"
   ```

3. **Clean up old sessions:**
   ```bash
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "DELETE FROM sessions WHERE expires_at < datetime('now');"
   ```

---

## Alert Issues

### Alerts Not Firing

**Symptoms:**
- Alert rule enabled
- Conditions met
- No notifications received

**Solutions:**

1. **Verify alert rule is enabled:**
   ```bash
   Navigate to: Alerting → Rules
   # Check "Enabled" toggle is ON
   ```

2. **Test notification channel:**
   ```bash
   Navigate to: Alerting → Channels → Your Channel → Test
   # Should receive test notification
   ```

3. **Check for active silences:**
   ```bash
   Navigate to: Alerting → Silences
   # Remove any silences matching your alert
   ```

4. **Review alert logs:**
   ```bash
   sudo journalctl -u openvox-webui | grep -i alert
   ```

5. **Verify alert conditions:**
   - Check thresholds are correct
   - Verify data meets conditions

### Webhook Alerts Failing

**Symptoms:**
- Test webhook succeeds
- Real alerts fail to deliver

**Solutions:**

1. **Check webhook endpoint availability:**
   ```bash
   curl -X POST https://your-webhook.example.com/alerts \
     -H "Content-Type: application/json" \
     -d '{"test": true}'
   ```

2. **Verify webhook timeout:**
   ```yaml
   # Increase timeout if webhook is slow
   timeout_secs: 30
   ```

3. **Check webhook logs on receiving end:**
   - Verify request is received
   - Check for authentication failures

4. **Review OpenVox logs:**
   ```bash
   sudo journalctl -u openvox-webui | grep webhook
   ```

### Too Many Alerts (Alert Fatigue)

**Solutions:**

1. **Adjust thresholds:**
   - Make conditions more specific
   - Increase duration before alerting

2. **Use silences during maintenance:**
   ```bash
   Navigate to: Alerting → Silences → Create Silence
   Duration: 2 hours
   Reason: Scheduled maintenance
   ```

3. **Group similar alerts:**
   - Combine related alert rules
   - Use alert aggregation

4. **Implement alert escalation:**
   - Info severity → Log only
   - Warning severity → Slack
   - Critical severity → Email + Slack + PagerDuty

---

## UI Issues

### Dark Mode Not Working

**Solutions:**

1. **Clear browser cache:**
   - Ctrl+Shift+Delete (or Cmd+Shift+Delete on Mac)
   - Clear localStorage

2. **Toggle theme manually:**
   ```bash
   Navigate to: Profile → Appearance → Select Theme
   ```

3. **Check localStorage:**
   ```javascript
   // Browser console
   localStorage.getItem('ov-theme')
   // Should be 'light' or 'dark'

   // Set manually if needed
   localStorage.setItem('ov-theme', 'dark')
   location.reload()
   ```

### Page Not Loading / Blank Screen

**Solutions:**

1. **Check browser console:**
   - F12 → Console tab
   - Look for JavaScript errors

2. **Verify API connectivity:**
   ```bash
   # Check if API is responding
   curl https://openvox.example.com/api/v1/health
   ```

3. **Clear browser cache and cookies:**
   - Full browser cache clear
   - Delete all site data

4. **Try different browser:**
   - Test in Chrome/Firefox/Safari
   - Check if issue is browser-specific

5. **Check for browser extensions:**
   - Ad blockers may interfere
   - Try in incognito/private mode

### Elements Not Clickable / UI Frozen

**Solutions:**

1. **Check for modal overlays:**
   - Press Escape key
   - Look for hidden modal dialogs

2. **Refresh the page:**
   - Ctrl+R (or Cmd+R)
   - Hard refresh: Ctrl+Shift+R

3. **Check browser console for errors:**
   - F12 → Console
   - Look for React errors

---

## Database Issues

### Database Locked

**Symptoms:**
- "Database is locked" error
- Cannot create/update resources

**Solutions:**

1. **Check for long-running queries:**
   ```bash
   # Stop service
   sudo systemctl stop openvox-webui

   # Check for locks
   sudo lsof | grep openvox.db

   # Kill any hanging processes
   sudo kill -9 <PID>

   # Restart service
   sudo systemctl start openvox-webui
   ```

2. **Increase timeout:**
   ```yaml
   # config.yaml
   database:
     busy_timeout: 5000  # Milliseconds
   ```

3. **Switch to Write-Ahead Logging (WAL):**
   ```bash
   sqlite3 /var/lib/openvox-webui/openvox.db "PRAGMA journal_mode=WAL;"
   ```

### Database Corruption

**Symptoms:**
- "Database disk image is malformed"
- Application crashes on startup

**Solutions:**

1. **Verify corruption:**
   ```bash
   sqlite3 /var/lib/openvox-webui/openvox.db "PRAGMA integrity_check;"
   ```

2. **Attempt repair:**
   ```bash
   # Backup first!
   cp /var/lib/openvox-webui/openvox.db /var/lib/openvox-webui/openvox.db.backup

   # Dump and restore
   sqlite3 /var/lib/openvox-webui/openvox.db ".dump" | sqlite3 openvox_repaired.db

   # Replace
   mv openvox_repaired.db /var/lib/openvox-webui/openvox.db
   ```

3. **Restore from backup:**
   ```bash
   # Use your most recent backup
   cp /path/to/backup/openvox.db /var/lib/openvox-webui/openvox.db
   sudo systemctl restart openvox-webui
   ```

---

## General Debugging

### Enable Debug Logging

```yaml
# config.yaml
logging:
  level: "debug"  # Change from "info"
  format: "json"
  target: "file"
  log_dir: "/var/log/openvox/webui"
```

Restart service:
```bash
sudo systemctl restart openvox-webui
```

View logs:
```bash
# Real-time logs
sudo journalctl -u openvox-webui -f

# Specific time range
sudo journalctl -u openvox-webui --since "1 hour ago"

# Search for errors
sudo journalctl -u openvox-webui | grep -i error

# File logs
tail -f /var/log/openvox/webui/openvox-webui.log
```

### Check Service Status

```bash
# Service status
sudo systemctl status openvox-webui

# If failed to start
sudo journalctl -u openvox-webui -n 50 --no-pager

# Check if port is in use
sudo netstat -tlnp | grep :443
```

### Verify Configuration

```bash
# Validate YAML syntax
yamllint /etc/openvox-webui/config.yaml

# Check for common issues
grep -E '^\s*\t' /etc/openvox-webui/config.yaml
# Should be empty (no tabs, use spaces)
```

### Test API Endpoints

```bash
# Health check
curl https://openvox.example.com/api/v1/health

# Auth test
curl -X POST https://openvox.example.com/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"test"}'

# Nodes (with token)
curl -H "Authorization: Bearer <token>" \
  https://openvox.example.com/api/v1/nodes
```

### Collect Debug Information

For support requests, collect:

1. **Version info:**
   ```bash
   openvox-webui --version
   # Or check /api/v1/health endpoint
   ```

2. **Configuration (sanitized):**
   ```bash
   # Remove secrets first!
   cat /etc/openvox-webui/config.yaml | \
     sed 's/password:.*/password: [REDACTED]/' | \
     sed 's/secret:.*/secret: [REDACTED]/'
   ```

3. **Logs:**
   ```bash
   sudo journalctl -u openvox-webui --since "1 hour ago" > openvox-debug.log
   ```

4. **System info:**
   ```bash
   uname -a
   cat /etc/os-release
   systemctl --version
   ```

5. **Database info:**
   ```bash
   sqlite3 /var/lib/openvox-webui/openvox.db \
     "SELECT name FROM sqlite_master WHERE type='table';"
   ```

---

## Getting Help

If you cannot resolve your issue:

1. **Check existing issues:**
   - GitHub Issues: https://github.com/ffquintella/openvox-webui/issues
   - Search for similar problems

2. **Create a new issue:**
   - Use bug report template
   - Include debug information above
   - Describe steps to reproduce

3. **Community support:**
   - GitHub Discussions: https://github.com/ffquintella/openvox-webui/discussions
   - Join community chat (if available)

4. **Documentation:**
   - [User Guide](USER_GUIDE.md)
   - [Configuration Guide](CONFIGURATION.md)
   - [Installation Guide](INSTALLATION.md)
