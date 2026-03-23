# Unix Socket Client Examples

Command-line examples for testing the WiFi Commissioning Service REST API via Unix domain socket.

## Quick Start

### Using the Helper Script

The `wifi-client.sh` script provides a simple command-line interface:

```bash
# Start WiFi scan
./wifi-client.sh scan

# List available networks
./wifi-client.sh list

# Connect to a network (PSK = 64 hex chars = hex-encoded 32 bytes)
./wifi-client.sh connect "MyNetwork" "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

# Check connection status
./wifi-client.sh status

# Get service version
./wifi-client.sh version

# Disconnect
./wifi-client.sh disconnect

# List networks saved in wpa_supplicant config
./wifi-client.sh list-saved

# Remove a saved network (for re-commissioning / credential rotation)
./wifi-client.sh forget "MyNetwork"
```

By default, the script uses `/run/wifi-commissioning-service/wlan0/api.sock`. Override with:
```bash
WIFI_SOCKET_PATH=/tmp/wifi.sock ./wifi-client.sh scan
```

### Raw curl Commands

All examples below use the default socket path. Add `-v` for verbose output.

## REST API

The Unix socket transport serves an HTTP REST API. All responses are JSON.

### Endpoints

| Method | Path | Description |
| ------ | ---- | ----------- |
| POST | `/api/v1/scan` | Start WiFi scan |
| GET | `/api/v1/scan/results` | Get scan results |
| POST | `/api/v1/connect` | Connect to network |
| POST | `/api/v1/disconnect` | Disconnect |
| GET | `/api/v1/status` | Connection status |
| GET | `/api/v1/version` | Get service version |
| GET | `/api/v1/networks` | List saved networks |
| POST | `/api/v1/networks/forget` | Remove a saved network by SSID |

## Available Endpoints

### 1. Start WiFi Scan

```bash
curl -X POST --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  http://localhost/api/v1/scan
```

Response (202 Accepted):
```json
{
  "status": "ok",
  "state": "scanning"
}
```

### 2. Get Scan Results

```bash
curl --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  http://localhost/api/v1/scan/results
```

Response (200 OK) — scan finished:
```json
{
  "status": "ok",
  "state": "finished",
  "networks": [
    {
      "ssid": "MyNetwork",
      "mac": "aa:bb:cc:dd:ee:ff",
      "ch": 6,
      "rssi": -45
    },
    {
      "ssid": "GuestNetwork",
      "mac": "11:22:33:44:55:66",
      "ch": 11,
      "rssi": -67
    }
  ]
}
```

Response (200 OK) — scan in progress:
```json
{
  "status": "ok",
  "state": "scanning",
  "networks": []
}
```

Pretty-print with `jq`:
```bash
curl -s --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  http://localhost/api/v1/scan/results | jq '.networks[] | {ssid, rssi, channel}'
```

### 3. Connect to WiFi Network

Connect to a network with SSID and pre-shared key (64 hex characters):

```bash
curl -X POST --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  -H "Content-Type: application/json" \
  -d '{
    "ssid": "MyNetwork",
    "psk": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  }' \
  http://localhost/api/v1/connect
```

Response (202 Accepted):
```json
{
  "status": "ok",
  "state": "connecting"
}
```

### 4. Check Connection Status

```bash
curl --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  http://localhost/api/v1/status
```

Response (200 OK):
```json
{
  "status": "ok",
  "state": "connected",
  "ssid": "MyNetwork",
  "ip_address": "192.168.1.100",
  "interface_name": "wlan0"
}
```

Possible states: `idle`, `connecting`, `connected`, `failed`

### 5. Get Service Version

```bash
curl --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  http://localhost/api/v1/version
```

Response (200 OK):
```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

### 6. Disconnect from Network

```bash
curl -X POST --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  http://localhost/api/v1/disconnect
```

Response (200 OK):
```json
{
  "status": "ok"
}
```

### 7. List Saved Networks

Returns all networks currently saved in `wpa_supplicant.conf`:

```bash
curl --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  http://localhost/api/v1/networks
```

Response (200 OK):
```json
{
  "status": "ok",
  "networks": [
    { "ssid": "HomeNetwork", "flags": "[CURRENT]" },
    { "ssid": "OfficeNetwork", "flags": "" }
  ]
}
```

### 8. Forget a Saved Network

Remove a network by SSID and persist the change. Idempotent — succeeds even if the SSID is not found.

```bash
curl -X POST --unix-socket /run/wifi-commissioning-service/wlan0/api.sock \
  -H "Content-Type: application/json" \
  -d '{"ssid": "OldNetwork"}' \
  http://localhost/api/v1/networks/forget
```

Response (200 OK):
```json
{
  "status": "ok"
}
```

Typical re-commissioning workflow:
```bash
SOCK=/run/wifi-commissioning-service/wlan0/api.sock

# 1. Inspect what is saved
curl -s --unix-socket $SOCK http://localhost/api/v1/networks | jq '.'

# 2. Remove the stale credential
curl -s -X POST --unix-socket $SOCK \
  -H "Content-Type: application/json" \
  -d '{"ssid":"OldNetwork"}' \
  http://localhost/api/v1/networks/forget

# 3. Commission with new credential
curl -s -X POST --unix-socket $SOCK \
  -H "Content-Type: application/json" \
  -d '{"ssid":"OldNetwork","psk":"<new-64-hex-char-psk>"}' \
  http://localhost/api/v1/connect
```

## Error Responses

Errors return the appropriate HTTP status code with a JSON body:

| HTTP Status | Error Code | Condition |
| ----------- | ---------- | --------- |
| 400 | `invalid_params` | Invalid request body (e.g., bad PSK format) |
| 409 | `operation_in_progress` | Scan or connect already running |
| 409 | `invalid_state` | No scan has been started (idle/error state) |
| 502 | `backend_error` | WiFi backend failure |

Example error response (409 Conflict):
```json
{
  "error": "operation_in_progress",
  "message": "Operation already in progress"
}
```

## Complete Workflow Example

```bash
SOCK=/run/wifi-commissioning-service/wlan0/api.sock

# 1. Start scan
curl -s -X POST --unix-socket $SOCK http://localhost/api/v1/scan

# 2. Wait for scan to complete
sleep 3

# 3. Get networks
curl -s --unix-socket $SOCK http://localhost/api/v1/scan/results | jq '.'

# 4. Connect to network
curl -s -X POST --unix-socket $SOCK \
  -H "Content-Type: application/json" \
  -d '{"ssid":"MyNetwork","psk":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}' \
  http://localhost/api/v1/connect

# 5. Check status
curl -s --unix-socket $SOCK http://localhost/api/v1/status | jq '.'
```

## Requirements

- `curl` with Unix socket support
- `jq` (optional, for pretty JSON output)
- Running WiFi commissioning service with Unix socket enabled

## Socket Path Configuration

### Production (systemd socket activation)

In production, systemd manages the socket at a fixed path:

```
/run/wifi-commissioning-service/wlan0/api.sock
```

The service automatically detects and uses the systemd-provided socket.

### Testing/Development (standalone mode)

For testing without systemd, the service creates its own socket:

```bash
# Default location (standalone)
wifi-commissioning-service -i wlan0 -s "secret" --socket-path /tmp/wifi.sock

# Custom location
wifi-commissioning-service -i wlan0 -s "secret" --socket-path /tmp/custom.sock
```

**Note:** Standalone mode is for testing only. Production deployments should use systemd socket activation.

## Troubleshooting

**"Couldn't connect to server"**

- Check the service is running: `systemctl status wifi-commissioning-service@wlan0`
- Check the socket is active: `systemctl status wifi-commissioning-service@wlan0.socket`
- Verify socket exists: `ls -l /run/wifi-commissioning-service/wlan0/api.sock`
- Check permissions on the socket file

**"Permission denied"**
- Add your user to the appropriate group: `sudo usermod -a -G wpa_supplicant $USER`
- Or run with sudo: `sudo ./wifi-client.sh scan`

## Advanced: Batch Operations

Process multiple requests in a script:

```bash
#!/bin/bash
SOCK="/run/wifi-commissioning-service/wlan0/api.sock"
URL="http://localhost/api/v1"

# Scan and wait
curl -s -X POST --unix-socket "$SOCK" "$URL/scan"
sleep 3

# Get networks and find strongest
networks=$(curl -s --unix-socket "$SOCK" "$URL/scan/results")
ssid=$(echo "$networks" | jq -r '.networks | sort_by(.rssi) | reverse | .[0].ssid')

echo "Connecting to strongest network: $ssid"
curl -s -X POST --unix-socket "$SOCK" \
  -H "Content-Type: application/json" \
  -d "{\"ssid\":\"$ssid\",\"psk\":\"$PSK\"}" \
  "$URL/connect"
```

## See Also

- [curl Unix socket documentation](https://curl.se/docs/manpage.html#--unix-socket)
- [Web BLE Client](../web-ble-client/README.md) - Browser-based BLE interface
