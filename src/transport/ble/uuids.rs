//! BLE GATT UUIDs — backwards compatible with wifi-commissioning-gatt-service

use uuid::Uuid;

// Service UUIDs (d69a37ee-1d8a-4329-bd24-25db4af3c8xx family)

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

// Characteristic UUIDs (811ce666-22e0-4a6d-a50f-0c78e076faax family)

/// Authorization key characteristic — write 32-byte SHA3-256 hash
pub const AUTH_KEY_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa6,
]);

/// Scan status characteristic — read/write/notify (state machine)
pub const SCAN_STATUS_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa0,
]);

/// Scan select characteristic — read chunk count, write index
pub const SCAN_SELECT_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa1,
]);

/// Scan result characteristic — read selected 100-byte chunk
pub const SCAN_RESULT_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa2,
]);

/// Connect state characteristic — read/write/notify (state machine)
pub const CONNECT_STATE_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa3,
]);

/// Connect SSID characteristic — read/write
pub const CONNECT_SSID_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa4,
]);

/// Connect PSK characteristic — write-only 32 bytes
pub const CONNECT_PSK_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa5,
]);

// ── Network management service UUIDs ────────────────────────────────────────

/// Network management service UUID
pub const NET_MGMT_SERVICE_UUID: Uuid = Uuid::from_bytes([
    0xd6, 0x9a, 0x37, 0xee, 0x1d, 0x8a, 0x43, 0x29, 0xbd, 0x24, 0x25, 0xdb, 0x4a, 0xf3, 0xc8, 0x66,
]);

/// Saved-network list status characteristic — read/write/notify (state machine)
pub const NET_LIST_STATUS_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa7,
]);

/// Saved-network list select characteristic — read chunk count, write index
pub const NET_LIST_SELECT_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa8,
]);

/// Saved-network list result characteristic — read selected 100-byte chunk
pub const NET_LIST_RESULT_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xa9,
]);

/// Forget network characteristic — write SSID bytes to remove the saved network
pub const NET_FORGET_CHAR_UUID: Uuid = Uuid::from_bytes([
    0x81, 0x1c, 0xe6, 0x66, 0x22, 0xe0, 0x4a, 0x6d, 0xa5, 0x0f, 0x0c, 0x78, 0xe0, 0x76, 0xfa, 0xaa,
]);

/// Maximum chunk size for scan result reads
pub const SCAN_RESULT_CHUNK_SIZE: usize = 100;

/// Maximum SSID length in bytes
pub const SSID_MAX_LENGTH: usize = 32;

/// PSK length in bytes (fixed)
pub const PSK_LENGTH: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_uuid_format() {
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
        assert_eq!(
            NET_MGMT_SERVICE_UUID.to_string(),
            "d69a37ee-1d8a-4329-bd24-25db4af3c866"
        );
    }

    #[test]
    fn test_characteristic_uuid_format() {
        assert_eq!(
            AUTH_KEY_CHAR_UUID.to_string(),
            "811ce666-22e0-4a6d-a50f-0c78e076faa6"
        );
        assert_eq!(
            SCAN_STATUS_CHAR_UUID.to_string(),
            "811ce666-22e0-4a6d-a50f-0c78e076faa0"
        );
        assert_eq!(
            SCAN_SELECT_CHAR_UUID.to_string(),
            "811ce666-22e0-4a6d-a50f-0c78e076faa1"
        );
        assert_eq!(
            SCAN_RESULT_CHAR_UUID.to_string(),
            "811ce666-22e0-4a6d-a50f-0c78e076faa2"
        );
        assert_eq!(
            CONNECT_STATE_CHAR_UUID.to_string(),
            "811ce666-22e0-4a6d-a50f-0c78e076faa3"
        );
        assert_eq!(
            CONNECT_SSID_CHAR_UUID.to_string(),
            "811ce666-22e0-4a6d-a50f-0c78e076faa4"
        );
        assert_eq!(
            CONNECT_PSK_CHAR_UUID.to_string(),
            "811ce666-22e0-4a6d-a50f-0c78e076faa5"
        );
    }

    #[test]
    fn test_characteristic_uuids_unique() {
        let uuids = [
            AUTH_KEY_CHAR_UUID,
            SCAN_STATUS_CHAR_UUID,
            SCAN_SELECT_CHAR_UUID,
            SCAN_RESULT_CHAR_UUID,
            CONNECT_STATE_CHAR_UUID,
            CONNECT_SSID_CHAR_UUID,
            CONNECT_PSK_CHAR_UUID,
            NET_LIST_STATUS_CHAR_UUID,
            NET_LIST_SELECT_CHAR_UUID,
            NET_LIST_RESULT_CHAR_UUID,
            NET_FORGET_CHAR_UUID,
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
