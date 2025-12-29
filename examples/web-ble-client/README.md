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

The client communicates with three BLE GATT services:

### Authorization Service (`d69a37ee-1d8a-4329-bd24-25db4af3c865`)
- **Auth Key** (`c866`): Write SHA3-256 hash of device secret for authentication

### Scan Service (`d69a37ee-1d8a-4329-bd24-25db4af3c863`)
- **Scan Control** (`c867`): Write `1` to start scanning
- **Scan State** (`c868`): Read/notify scan status (0=idle, 1=scanning, 2=complete, 3=error)
- **Scan Results** (`c869`): Read JSON array of discovered networks in 100-byte chunks

### Connect Service (`d69a37ee-1d8a-4329-bd24-25db4af3c864`)
- **Connect SSID** (`c86a`): Write SSID as UTF-8 string (accumulated over multiple writes)
- **Connect PSK** (`c86b`): Write 32-byte PSK derived via PBKDF2
- **Connect Control** (`c86c`): Write `1` to connect, `2` to disconnect
- **Connect State** (`c86d`): Read/notify connection status (0=idle, 1=connecting, 2=connected, 3=error)

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
