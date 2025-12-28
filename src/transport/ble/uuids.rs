//! BLE GATT UUIDs for backwards compatibility

use uuid::Uuid;

/// Authorization service UUID
pub const AUTHORIZATION_SERVICE_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x65,
]);

/// Scan service UUID
pub const SCAN_SERVICE_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x63,
]);

/// Connect service UUID
pub const CONNECT_SERVICE_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x64,
]);

// Authorization service characteristics
/// Authorization key write characteristic (32-byte SHA3 hash)
pub const AUTH_KEY_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x66,
]);

// Scan service characteristics
/// Scan control characteristic (write to start scan)
pub const SCAN_CONTROL_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x67,
]);

/// Scan state characteristic (read/notify)
pub const SCAN_STATE_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x68,
]);

/// Scan results characteristic (read in 100-byte chunks)
pub const SCAN_RESULTS_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x69,
]);

// Connect service characteristics
/// SSID write characteristic (accumulates partial writes)
pub const CONNECT_SSID_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x6a,
]);

/// PSK write characteristic (32 bytes)
pub const CONNECT_PSK_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x6b,
]);

/// Connection control characteristic (write to connect/disconnect)
pub const CONNECT_CONTROL_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x6c,
]);

/// Connection state characteristic (read/notify)
pub const CONNECT_STATE_CHAR_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x6d,
]);

/// Maximum chunk size for BLE characteristics
pub const MAX_CHUNK_SIZE: usize = 100;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_format() {
        // Verify UUIDs are correctly formatted
        assert_eq!(
            AUTHORIZATION_SERVICE_UUID.to_string(),
            "d69a37ee-1d8a-4329-bd24-25db4af3c865"
        );
        assert_eq!(
            SCAN_SERVICE_UUID.to_string(),
            "d69a37ee-1d8a-4329-bd24-25db4af3c863"
        );
        assert_eq!(
            CONNECT_SERVICE_UUID.to_string(),
            "d69a37ee-1d8a-4329-bd24-25db4af3c864"
        );
    }

    #[test]
    fn test_characteristic_uuids_unique() {
        // Ensure all characteristic UUIDs are unique
        let uuids = [
            AUTH_KEY_CHAR_UUID,
            SCAN_CONTROL_CHAR_UUID,
            SCAN_STATE_CHAR_UUID,
            SCAN_RESULTS_CHAR_UUID,
            CONNECT_SSID_CHAR_UUID,
            CONNECT_PSK_CHAR_UUID,
            CONNECT_CONTROL_CHAR_UUID,
            CONNECT_STATE_CHAR_UUID,
        ];

        for (i, uuid1) in uuids.iter().enumerate() {
            for (j, uuid2) in uuids.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        uuid1, uuid2,
                        "UUIDs at positions {} and {} are not unique",
                        i, j
                    );
                }
            }
        }
    }
}
