#!/bin/bash
# WiFi Commissioning Unix Socket Client
# JSON-RPC 2.0 client for testing the Unix socket transport

set -e

SOCKET_PATH="${WIFI_SOCKET_PATH:-/var/run/wifi-commissioning.sock}"
REQUEST_ID=1

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
    scan                        Start WiFi scan
    list                        Get scan results
    connect <ssid> <password>   Connect to WiFi network
    disconnect                  Disconnect from WiFi
    status                      Get connection status

Environment:
    WIFI_SOCKET_PATH    Path to Unix socket (default: /var/run/wifi-commissioning.sock)

Examples:
    $0 scan
    $0 list
    $0 connect "MyNetwork" "MyPassword123"
    $0 disconnect
    $0 status
EOF
    exit 1
}

# JSON-RPC 2.0 request helper
jsonrpc_request() {
    local method="$1"
    local params="$2"

    if [ -z "$params" ] || [ "$params" = "null" ]; then
        request_body=$(cat <<EOF
{
    "jsonrpc": "2.0",
    "method": "$method",
    "id": $REQUEST_ID
}
EOF
)
    else
        request_body=$(cat <<EOF
{
    "jsonrpc": "2.0",
    "method": "$method",
    "params": $params,
    "id": $REQUEST_ID
}
EOF
)
    fi

    echo -e "${BLUE}→ Request:${NC}" >&2
    echo "$request_body" | jq '.' 2>/dev/null || echo "$request_body" >&2
    echo "" >&2

    response=$(curl -s --unix-socket "$SOCKET_PATH" \
        -H "Content-Type: application/json" \
        -d "$request_body" \
        http://localhost/)

    echo -e "${BLUE}← Response:${NC}" >&2
    echo "$response" | jq '.' 2>/dev/null || echo "$response" >&2
    echo "" >&2

    # Check for JSON-RPC error
    if echo "$response" | jq -e '.error' >/dev/null 2>&1; then
        error_msg=$(echo "$response" | jq -r '.error.message')
        error_code=$(echo "$response" | jq -r '.error.code')
        echo -e "${RED}✗ Error $error_code: $error_msg${NC}" >&2
        exit 1
    fi

    echo "$response"
    REQUEST_ID=$((REQUEST_ID + 1))
}

# Check if socket exists
check_socket() {
    if [ ! -S "$SOCKET_PATH" ]; then
        echo -e "${RED}✗ Socket not found: $SOCKET_PATH${NC}" >&2
        echo -e "${YELLOW}  Is the wifi-commissioning service running?${NC}" >&2
        exit 1
    fi
}

# Scan for WiFi networks
cmd_scan() {
    echo -e "${GREEN}Starting WiFi scan...${NC}"
    check_socket
    jsonrpc_request "scan" "null" >/dev/null
    echo -e "${GREEN}✓ Scan started${NC}"
    echo -e "${YELLOW}  Use '$0 list' to get results${NC}"
}

# List scan results
cmd_list() {
    echo -e "${GREEN}Fetching scan results...${NC}"
    check_socket
    response=$(jsonrpc_request "list_networks" "null")

    # Pretty print the networks
    networks=$(echo "$response" | jq -r '.result[]? | "\(.ssid)\t\(.signal)\t\(.security)"' 2>/dev/null)

    if [ -n "$networks" ]; then
        echo -e "${GREEN}✓ Available networks:${NC}"
        echo ""
        printf "%-32s %-10s %s\n" "SSID" "Signal" "Security"
        printf "%-32s %-10s %s\n" "----" "------" "--------"
        echo "$networks" | while IFS=$'\t' read -r ssid signal security; do
            printf "%-32s %-10s %s\n" "$ssid" "$signal" "$security"
        done
    else
        echo -e "${YELLOW}No networks found. Try running scan first.${NC}"
    fi
}

# Connect to WiFi
cmd_connect() {
    local ssid="$1"
    local password="$2"

    if [ -z "$ssid" ] || [ -z "$password" ]; then
        echo -e "${RED}✗ Usage: $0 connect <ssid> <password>${NC}" >&2
        exit 1
    fi

    echo -e "${GREEN}Connecting to '$ssid'...${NC}"
    check_socket

    params=$(jq -n --arg ssid "$ssid" --arg password "$password" \
        '{ssid: $ssid, password: $password}')

    jsonrpc_request "connect" "$params" >/dev/null
    echo -e "${GREEN}✓ Connection initiated${NC}"
    echo -e "${YELLOW}  Use '$0 status' to check connection status${NC}"
}

# Disconnect from WiFi
cmd_disconnect() {
    echo -e "${GREEN}Disconnecting...${NC}"
    check_socket
    jsonrpc_request "disconnect" "null" >/dev/null
    echo -e "${GREEN}✓ Disconnected${NC}"
}

# Get connection status
cmd_status() {
    echo -e "${GREEN}Fetching connection status...${NC}"
    check_socket
    response=$(jsonrpc_request "status" "null")

    state=$(echo "$response" | jq -r '.result.state' 2>/dev/null)
    ssid=$(echo "$response" | jq -r '.result.ssid // "N/A"' 2>/dev/null)

    echo -e "${GREEN}✓ Status:${NC}"
    echo "  State: $state"
    if [ "$ssid" != "N/A" ] && [ "$ssid" != "null" ]; then
        echo "  SSID:  $ssid"
    fi
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
        cmd_connect "$2" "$3"
        ;;
    disconnect)
        cmd_disconnect
        ;;
    status)
        cmd_status
        ;;
    -h|--help|help|"")
        usage
        ;;
    *)
        echo -e "${RED}✗ Unknown command: $1${NC}" >&2
        echo "" >&2
        usage
        ;;
esac
