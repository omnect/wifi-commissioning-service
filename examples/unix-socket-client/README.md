# Unix Socket Client Examples

Command-line examples for testing the WiFi Commissioning Service via Unix domain socket using JSON-RPC 2.0.

## Quick Start

### Using the Helper Script

The `wifi-client.sh` script provides a simple command-line interface:

```bash
# Start WiFi scan
./wifi-client.sh scan

# List available networks
./wifi-client.sh list

# Connect to a network
./wifi-client.sh connect "MyNetwork" "MyPassword123"

# Check connection status
./wifi-client.sh status

# Disconnect
./wifi-client.sh disconnect
```

By default, the script uses `/var/run/wifi-commissioning.sock`. Override with:
```bash
WIFI_SOCKET_PATH=/tmp/wifi.sock ./wifi-client.sh scan
```

### Raw curl Commands

All examples below use the default socket path. Add `-v` for verbose output.

## JSON-RPC 2.0 Protocol

All requests follow the JSON-RPC 2.0 specification:

```json
{
  "jsonrpc": "2.0",
  "method": "method_name",
  "params": { ... },
  "id": 1
}
```

Responses:
```json
{
  "jsonrpc": "2.0",
  "result": { ... },
  "id": 1
}
```

Errors:
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32600,
    "message": "Invalid Request"
  },
  "id": 1
}
```

## Available Methods

### 1. Scan for WiFi Networks

Start a WiFi scan in the background:

```bash
curl --unix-socket /var/run/wifi-commissioning.sock \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "scan",
    "id": 1
  }' \
  http://localhost/
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 1
}
```

### 2. List Available Networks

Retrieve scan results:

```bash
curl --unix-socket /var/run/wifi-commissioning.sock \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "list_networks",
    "id": 2
  }' \
  http://localhost/
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "ssid": "MyNetwork",
      "signal": -45,
      "security": "WPA2-PSK"
    },
    {
      "ssid": "GuestNetwork",
      "signal": -67,
      "security": "Open"
    }
  ],
  "id": 2
}
```

Pretty-print with `jq`:
```bash
curl -s --unix-socket /var/run/wifi-commissioning.sock \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"list_networks","id":2}' \
  http://localhost/ | jq '.result[] | {ssid, signal, security}'
```

### 3. Connect to WiFi Network

Connect to a network with SSID and password:

```bash
curl --unix-socket /var/run/wifi-commissioning.sock \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "connect",
    "params": {
      "ssid": "MyNetwork",
      "password": "MyPassword123"
    },
    "id": 3
  }' \
  http://localhost/
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 3
}
```

### 4. Check Connection Status

Get current connection state:

```bash
curl --unix-socket /var/run/wifi-commissioning.sock \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "status",
    "id": 4
  }' \
  http://localhost/
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "state": "connected",
    "ssid": "MyNetwork"
  },
  "id": 4
}
```

Possible states: `idle`, `connecting`, `connected`, `error`

### 5. Disconnect from Network

Disconnect from the current network:

```bash
curl --unix-socket /var/run/wifi-commissioning.sock \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "disconnect",
    "id": 5
  }' \
  http://localhost/
```

Response:
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 5
}
```

## Error Codes

The service returns standard JSON-RPC 2.0 error codes plus custom codes:

| Code | Message | Description |
|------|---------|-------------|
| -32700 | Parse error | Invalid JSON |
| -32600 | Invalid Request | Missing required fields |
| -32601 | Method not found | Unknown method name |
| -32602 | Invalid params | Invalid method parameters |
| -32603 | Internal error | Service internal error |
| -32001 | WiFi error | WiFi backend error (scan/connect failed) |
| -32002 | State error | Invalid state for operation |

Example error response:
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "WiFi scan failed: Device busy"
  },
  "id": 1
}
```

## Complete Workflow Example

```bash
# 1. Start scan
curl -s --unix-socket /var/run/wifi-commissioning.sock \
  -d '{"jsonrpc":"2.0","method":"scan","id":1}' \
  http://localhost/

# 2. Wait a moment for scan to complete
sleep 3

# 3. List networks
curl -s --unix-socket /var/run/wifi-commissioning.sock \
  -d '{"jsonrpc":"2.0","method":"list_networks","id":2}' \
  http://localhost/ | jq '.'

# 4. Connect to network
curl -s --unix-socket /var/run/wifi-commissioning.sock \
  -d '{"jsonrpc":"2.0","method":"connect","params":{"ssid":"MyNetwork","password":"MyPass"},"id":3}' \
  http://localhost/

# 5. Check status
curl -s --unix-socket /var/run/wifi-commissioning.sock \
  -d '{"jsonrpc":"2.0","method":"status","id":4}' \
  http://localhost/ | jq '.result'
```

## Requirements

- `curl` with Unix socket support
- `jq` (optional, for pretty JSON output)
- Running WiFi commissioning service with Unix socket enabled

## Socket Path Configuration

### Production (systemd socket activation)

In production, systemd manages the socket. The socket path is defined in the `.socket` unit file:

```bash
# Socket path pattern (where %i is the interface name)
/run/wifi-commissioning-%i.sock

# Example for wlan0
/run/wifi-commissioning-wlan0.sock
```

The service automatically detects and uses the systemd-provided socket.

### Testing/Development (standalone mode)

For testing without systemd, the service creates its own socket:

```bash
# Default location (standalone)
wifi-commissioning-service -i wlan0 -s "secret" --enable-unix-socket --socket-path /tmp/wifi.sock

# Custom location
wifi-commissioning-service -i wlan0 -s "secret" --enable-unix-socket --socket-path /tmp/custom.sock
```

**Note:** Standalone mode is for testing only. Production deployments should use systemd socket activation.

## Testing with Mock Service

For local testing without hardware, you can use `socat` to create a test socket:

```bash
# Terminal 1: Create echo server
socat UNIX-LISTEN:/tmp/test.sock,fork EXEC:'/bin/cat'

# Terminal 2: Test with curl
curl --unix-socket /tmp/test.sock -d '{"test":"data"}' http://localhost/
```

## Troubleshooting

**"Couldn't connect to server"**

- Check the service is running: `systemctl status wifi-commissioning-service@wlan0`
- Check the socket is active: `systemctl status wifi-commissioning-service@wlan0.socket`
- Verify socket exists: `ls -l /run/wifi-commissioning-wlan0.sock`
- Check permissions on the socket file

**"Permission denied"**
- Add your user to the appropriate group: `sudo usermod -a -G wpa_supplicant $USER`
- Or run with sudo: `sudo ./wifi-client.sh scan`

**"Method not found"**
- Check method name spelling
- Verify the service version supports the method

## Advanced: Batch Operations

Process multiple requests in a script:

```bash
#!/bin/bash
SOCKET="/var/run/wifi-commissioning.sock"

request() {
    curl -s --unix-socket "$SOCKET" \
        -H "Content-Type: application/json" \
        -d "$1" \
        http://localhost/
}

# Scan and wait
request '{"jsonrpc":"2.0","method":"scan","id":1}'
sleep 3

# Get networks and connect to strongest
networks=$(request '{"jsonrpc":"2.0","method":"list_networks","id":2}')
ssid=$(echo "$networks" | jq -r '.result | sort_by(.signal) | reverse | .[0].ssid')

echo "Connecting to strongest network: $ssid"
request "{\"jsonrpc\":\"2.0\",\"method\":\"connect\",\"params\":{\"ssid\":\"$ssid\",\"password\":\"$PASSWORD\"},\"id\":3}"
```

## See Also

- [JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [curl Unix socket documentation](https://curl.se/docs/manpage.html#--unix-socket)
- [Web BLE Client](../web-ble-client/README.md) - Browser-based BLE interface
