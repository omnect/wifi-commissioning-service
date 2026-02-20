# Web BLE Client

Browser-based testing client for the WiFi Commissioning Service BLE GATT interface.

## Overview

This Web Bluetooth client allows you to commission WiFi credentials to a device running the WiFi commissioning service through a Chromium-based browser. The client communicates directly with the BLE GATT server using the Web Bluetooth API.

## Requirements

- **Browser**: Chrome/Chromium 55 or later with Web Bluetooth API support
- **Connection**: HTTPS or localhost (required for Web Bluetooth API)
- **Hardware**: Bluetooth adapter and device running the WiFi commissioning service

## Quick Start

1. Start a local HTTP server in this directory:
   ```bash
   python3 -m http.server 8000
   ```

2. Navigate to `http://localhost:8000` in Chrome/Chromium

3. Enter the BLE secret (device ID by default) in the input field

4. Click "Connect Bluetooth Device" and select your device (prefix: `omnectWifiConfig`)

5. Once connected, click "Find Access Points" to scan for WiFi networks

6. Select a network, enter the password, and click "Send Access Point to device and connect"

## Protocol Overview

The client communicates with three BLE GATT services. Service UUIDs use the `d69a37ee-...-c8xx` family; characteristic UUIDs use the `811ce666-22e0-4a6d-a50f-0c78e076faax` family.

### Authorization Service (`d69a37ee-1d8a-4329-bd24-25db4af3c865`)
- **Auth Key** (`faa6`): Write SHA3-256 hash of device secret for authentication

### Scan Service (`d69a37ee-1d8a-4329-bd24-25db4af3c863`)
- **Scan Status** (`faa0`): Read/write/notify — write `1` to start scanning, write `0` to reset; notifies on state change (0=idle, 1=scanning, 2=complete, 3=error)
- **Scan Select** (`faa1`): Read chunk count (u8); write desired chunk index (u8) to select it
- **Scan Result** (`faa2`): Read — returns the selected 100-byte JSON chunk

### Connect Service (`d69a37ee-1d8a-4329-bd24-25db4af3c864`)
- **Connect SSID** (`faa4`): Read/write SSID as UTF-8 string (accumulated over multiple writes)
- **Connect PSK** (`faa5`): Write-only 32-byte raw PSK derived via PBKDF2
- **Connect State** (`faa3`): Read/write/notify — write `1` to connect, write `0` to disconnect; notifies on state change (0=idle, 1=connecting, 2=connected, 3=error)

### Network Management Service (`d69a37ee-1d8a-4329-bd24-25db4af3c866`)

- **Net List Status** (`faa7`): Read/write/notify — write `1` to trigger refresh; poll or wait for notify until state becomes `2` (finished) or `3` (error); write `0` to reset to idle. States: 0=idle, 1=loading, 2=finished, 3=error
- **Net List Select** (`faa8`): Read chunk count (u8); write desired chunk index (u8) to select it
- **Net List Result** (`faa9`): Read — returns the selected 100-byte JSON chunk. Reassemble all chunks to get the full `[{"ssid":"...","flags":"..."},...]` JSON array
- **Net Forget** (`faaa`): Write-only — write a UTF-8-encoded SSID to remove that network from the saved configuration (idempotent; no error if not found)

## Authentication

The client derives the authentication key using SHA3-256:

```javascript
authKey = SHA3-256(bleSecret)
```

The default `bleSecret` is the device ID, which can be obtained via `omnect_get_deviceid.sh` on the device.

## WiFi Password Processing

WiFi passwords are converted to 32-byte PSKs using PBKDF2:

```javascript
psk = PBKDF2(password, ssid, 4096 iterations, 256 bits)
```

This matches the WPA2-PSK key derivation standard.

## Files

- `index.html` - Main UI with connection controls
- `client.js` - BLE GATT client implementation
- `sha3.js` - SHA3-256 cryptographic library
- `pbkdf2.js` - PBKDF2 key derivation function
- `sha1.js` - SHA-1 library (dependency for PBKDF2)

## Known Limitations

- Web Bluetooth API is only available in Chromium-based browsers
- HTTPS or localhost required (Web Bluetooth security policy)
- Some platforms have limited BLE GATT support in browsers
- Maximum characteristic write size is 512 bytes (enforced by Web Bluetooth API)

## Troubleshooting

**"Web Bluetooth API is not available"**
- Enable "Experimental Web Platform" features in `chrome://flags/#enable-experimental-web-platform-features`
- Ensure you're using Chrome/Chromium 55+

**Cannot find device**
- Verify the device is advertising with prefix `omnectWifiConfig`
- Check Bluetooth is enabled on your computer
- Ensure the device isn't already connected to another client

**Authentication fails**
- Verify you're using the correct device ID/secret
- Check that the device is running the WiFi commissioning service
- Ensure the authorization service is accessible

## License

This work is based on Google Chrome Team examples and has been modified for WiFi commissioning. Available under the Apache License, Version 2.0.
