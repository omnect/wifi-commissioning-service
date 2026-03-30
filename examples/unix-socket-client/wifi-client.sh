#!/bin/bash
# WiFi Commissioning Unix Socket Client
# REST API client for testing the Unix socket transport

set -euo pipefail

SOCKET_PATH="${WIFI_SOCKET_PATH:-/run/wifi-commissioning-service/wlan0/api.sock}"
BASE_URL="http://localhost/api/v1"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

usage() {
    cat <<EOF
Usage: $0 [command] [arguments]

Commands:
    scan                    Start WiFi scan
    list                    Get scan results
    connect <ssid> <psk>    Connect to WiFi network (PSK = 64 hex chars)
    disconnect              Disconnect from WiFi
    status                  Get connection status
    version                 Get service version
    list-saved              List saved networks from wpa_supplicant config
    forget <ssid>           Remove a saved network by SSID

Environment:
    WIFI_SOCKET_PATH    Path to Unix socket (default: /run/wifi-commissioning-service/wlan0/api.sock)

Examples:
    $0 scan
    $0 list
    $0 connect "MyNetwork" "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    $0 disconnect
    $0 status
    $0 version
    $0 list-saved
    $0 forget "MyNetwork"
EOF
    exit 1
}

# REST API request helper
api_request() {
    local method="$1"
    local path="$2"
    local body="$3"

    local curl_args=(
        -s
        -w "\n%{http_code}"
        --unix-socket "$SOCKET_PATH"
    )

    if [ "$method" != "GET" ]; then
        curl_args+=(-X "$method")
    fi

    if [ -n "$body" ]; then
        curl_args+=(-H "Content-Type: application/json" -d "$body")
    fi

    curl_args+=("${BASE_URL}${path}")

    echo -e "${BLUE}→ ${method} ${path}${NC}" >&2
    if [ -n "$body" ]; then
        echo "$body" | jq '.' 2>/dev/null >&2 || echo "$body" >&2
    fi
    echo "" >&2

    local output
    output=$(curl "${curl_args[@]}")

    local http_code
    http_code=$(echo "$output" | tail -n1)
    local response
    response=$(echo "$output" | sed '$d')

    echo -e "${BLUE}← ${http_code}${NC}" >&2
    echo "$response" | jq '.' 2>/dev/null >&2 || echo "$response" >&2
    echo "" >&2

    # Check for error status codes
    if [ "$http_code" -ge 400 ]; then
        local error_msg
        error_msg=$(echo "$response" | jq -r '.message // .error // "Unknown error"' 2>/dev/null)
        echo -e "${RED}Error ${http_code}: ${error_msg}${NC}" >&2
        exit 1
    fi

    echo "$response"
}

# Check if socket exists
check_socket() {
    if [ ! -S "$SOCKET_PATH" ]; then
        echo -e "${RED}Socket not found: $SOCKET_PATH${NC}" >&2
        echo -e "${YELLOW}  Is the wifi-commissioning service running?${NC}" >&2
        exit 1
    fi
}

# Scan for WiFi networks
cmd_scan() {
    echo -e "${GREEN}Starting WiFi scan...${NC}"
    check_socket
    api_request POST "/scan" >/dev/null
    echo -e "${GREEN}Scan started${NC}"
    echo -e "${YELLOW}  Use '$0 list' to get results${NC}"
}

# List scan results
cmd_list() {
    echo -e "${GREEN}Fetching scan results...${NC}"
    check_socket
    response=$(api_request GET "/scan/results")

    networks=$(echo "$response" | jq -r '.networks[]? | "\(.ssid)\t\(.rssi)\t\(.ch)"' 2>/dev/null)

    if [ -n "$networks" ]; then
        echo -e "${GREEN}Available networks:${NC}"
        echo ""
        printf "%-32s %-10s %s\n" "SSID" "RSSI" "Channel"
        printf "%-32s %-10s %s\n" "----" "----" "-------"
        echo "$networks" | while IFS=$'\t' read -r ssid rssi ch; do
            printf "%-32s %-10s %s\n" "$ssid" "$rssi" "$ch"
        done
    else
        echo -e "${YELLOW}No networks found. Try running scan first.${NC}"
    fi
}

# Connect to WiFi
cmd_connect() {
    local ssid="$1"
    local psk="$2"

    if [ -z "$ssid" ] || [ -z "$psk" ]; then
        echo -e "${RED}Usage: $0 connect <ssid> <psk>${NC}" >&2
        echo -e "${YELLOW}  PSK must be 64 hex characters (hex-encoded 32 bytes)${NC}" >&2
        exit 1
    fi

    echo -e "${GREEN}Connecting to '$ssid'...${NC}"
    check_socket

    body=$(jq -n --arg ssid "$ssid" --arg psk "$psk" '{ssid: $ssid, psk: $psk}')
    api_request POST "/connect" "$body" >/dev/null
    echo -e "${GREEN}Connection initiated${NC}"
    echo -e "${YELLOW}  Use '$0 status' to check connection status${NC}"
}

# Disconnect from WiFi
cmd_disconnect() {
    echo -e "${GREEN}Disconnecting...${NC}"
    check_socket
    api_request POST "/disconnect" >/dev/null
    echo -e "${GREEN}Disconnected${NC}"
}

# Get connection status
cmd_status() {
    echo -e "${GREEN}Fetching connection status...${NC}"
    check_socket
    response=$(api_request GET "/status")

    state=$(echo "$response" | jq -r '.state' 2>/dev/null)
    ssid=$(echo "$response" | jq -r '.ssid // "N/A"' 2>/dev/null)
    ip=$(echo "$response" | jq -r '.ip_address // "N/A"' 2>/dev/null)
    iface=$(echo "$response" | jq -r '.interface_name' 2>/dev/null)

    echo -e "${GREEN}Status:${NC}"
    echo "  Interface: $iface"
    echo "  State:     $state"
    if [ "$ssid" != "N/A" ] && [ "$ssid" != "null" ]; then
        echo "  SSID:      $ssid"
    fi
    if [ "$ip" != "N/A" ] && [ "$ip" != "null" ]; then
        echo "  IP:        $ip"
    fi
}

# Get service version
cmd_version() {
    echo -e "${GREEN}Fetching service version...${NC}"
    check_socket
    response=$(api_request GET "/version")

    version=$(echo "$response" | jq -r '.version' 2>/dev/null)
    echo -e "${GREEN}Version:${NC} $version"
}

# List saved networks from wpa_supplicant config
cmd_list_saved() {
    echo -e "${GREEN}Fetching saved networks...${NC}"
    check_socket
    response=$(api_request GET "/networks")

    networks=$(echo "$response" | jq -r '.networks[]? | "\(.ssid)\t\(.flags)"' 2>/dev/null)

    if [ -n "$networks" ]; then
        echo -e "${GREEN}Saved networks:${NC}"
        echo ""
        printf "%-32s %s\n" "SSID" "Flags"
        printf "%-32s %s\n" "----" "-----"
        echo "$networks" | while IFS=$'\t' read -r ssid flags; do
            printf "%-32s %s\n" "$ssid" "$flags"
        done
    else
        echo -e "${YELLOW}No saved networks found.${NC}"
    fi
}

# Remove a saved network by SSID
cmd_forget() {
    local ssid="$1"

    if [ -z "$ssid" ]; then
        echo -e "${RED}Usage: $0 forget <ssid>${NC}" >&2
        exit 1
    fi

    echo -e "${GREEN}Forgetting network '$ssid'...${NC}"
    check_socket

    body=$(jq -n --arg ssid "$ssid" '{ssid: $ssid}')
    api_request POST "/networks/forget" "$body" >/dev/null
    echo -e "${GREEN}Network '$ssid' removed from saved config${NC}"
}

# Main command dispatcher
case "${1:-}" in
    scan)
        cmd_scan
        ;;
    list)
        cmd_list
        ;;
    connect)
        cmd_connect "${2:-}" "${3:-}"
        ;;
    disconnect)
        cmd_disconnect
        ;;
    status)
        cmd_status
        ;;
    version)
        cmd_version
        ;;
    list-saved)
        cmd_list_saved
        ;;
    forget)
        cmd_forget "${2:-}"
        ;;
    -h|--help|help|"")
        usage
        ;;
    *)
        echo -e "${RED}Unknown command: $1${NC}" >&2
        echo "" >&2
        usage
        ;;
esac
