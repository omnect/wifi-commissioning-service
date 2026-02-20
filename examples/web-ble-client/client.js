// This work is based on the example 'Read Characteristic Value Changed' 
// contributed by the Google Chrome Team at 
// https://googlechrome.github.io/samples/web-bluetooth/
// It has been modified by the Silicon Labs Apps Team to support an 
// application of Wi-Fi commissioning using Web BLE.
// It is made available as an example under the terms of the 
// Apache License, Version 2.0


// Wi-Fi Scanner Service BLE GATT Services And Characteristic UUIDs
const SVC_WIFI_SCANNER_UUID             = 'd69a37ee-1d8a-4329-bd24-25db4af3c863';
const CHR_WIFI_SCANNER_STATE_UUID       = '811ce666-22e0-4a6d-a50f-0c78e076faa0';
const CHR_WIFI_SCANNER_RESULT_UUID      = '811ce666-22e0-4a6d-a50f-0c78e076faa2';
const CHR_WIFI_SCANNER_SELECT_UUID      = '811ce666-22e0-4a6d-a50f-0c78e076faa1';

// Wi-Fi Configurator Service BLE GATT Services And Characteristic UUIDs
const SVC_WIFI_CONFIG_UUID              = 'd69a37ee-1d8a-4329-bd24-25db4af3c864';
const CHR_WIFI_CONFIG_STATE_UUID        = '811ce666-22e0-4a6d-a50f-0c78e076faa3';
const CHR_WIFI_CONFIG_SSID_UUID         = '811ce666-22e0-4a6d-a50f-0c78e076faa4';
const CHR_WIFI_CONFIG_PASSWORD_UUID     = '811ce666-22e0-4a6d-a50f-0c78e076faa5';

const SVC_WIFI_AUTH_UUID                = 'd69a37ee-1d8a-4329-bd24-25db4af3c865';
const CHR_WIFI_AUTH_KEY_UUID            = '811ce666-22e0-4a6d-a50f-0c78e076faa6';

// Network Management Service BLE GATT Services And Characteristic UUIDs
const SVC_NET_MGMT_UUID                 = 'd69a37ee-1d8a-4329-bd24-25db4af3c866';
const CHR_NET_LIST_STATUS_UUID          = '811ce666-22e0-4a6d-a50f-0c78e076faa7';
const CHR_NET_LIST_SELECT_UUID          = '811ce666-22e0-4a6d-a50f-0c78e076faa8';
const CHR_NET_LIST_RESULT_UUID          = '811ce666-22e0-4a6d-a50f-0c78e076faa9';
const CHR_NET_FORGET_UUID               = '811ce666-22e0-4a6d-a50f-0c78e076faaa';

// Network Management State Machine States
const NET_LIST_STATE_IDLE     = 0;
const NET_LIST_STATE_LOADING  = 1;
const NET_LIST_STATE_FINISHED = 2;
const NET_LIST_STATE_ERROR    = 3;

// Wi-Fi Scanner State Machine States
const WIFI_SCANNER_STATE_IDLE     = 0;
const WIFI_SCANNER_STATE_SCAN     = 1;
const WIFI_SCANNER_STATE_SCANNED  = 2;
const WIFI_SCANNER_STATE_ERROR    = 3;

// Wi-Fi Config State Machine States
const WIFI_CONFIG_STATE_IDLE      = 0;
const WIFI_CONFIG_STATE_CONNECT   = 1;
const WIFI_CONFIG_STATE_JOINED    = 2;
const WIFI_CONFIG_STATE_ERROR     = 3;

// Reconnect configuration.
// The RPi BCM43xxx chip shares its RF path between WiFi and Bluetooth.
// A WiFi scan disrupts the BLE connection; retrying immediately fails
// because the scan is still running or the radio hasn't settled yet.
const RECONNECT_DELAY_MS = 3000;
const MAX_RECONNECT_ATTEMPTS = 3;

// Global Variables
var bleSecret = 'some-secret';
var bluetoothDevice;
var wifiScannerStateCharacteristic;
var wifiScannerAP_Result_Characteristic;
var wifiScannerAP_Select_Characteristic;
var wifiConfigStateCharacteristic;
var wifiConfigSSIDCharacteristic;
var wifiConfigPskCharacteristic;
var wifiAuthKeyCharacteristic;
var netListStatusCharacteristic;
var netListSelectCharacteristic;
var netListResultCharacteristic;
var netForgetCharacteristic;
var accessPointsObj = [];


// This function requests BLE devices nearby 
// with the device prefix name 'omnectWifiConfig'.
async function requestDevice() {
  log('> Requesting Bluetooth Devices omnectWifiConfig*...');
  bleSecret = document.querySelector('#txtSecret').value;
  bluetoothDevice = await navigator.bluetooth.requestDevice({
      filters: [{namePrefix: 'omnectWifiConfig'}],
      optionalServices: [SVC_WIFI_SCANNER_UUID, SVC_WIFI_CONFIG_UUID, SVC_WIFI_AUTH_UUID, SVC_NET_MGMT_UUID]
      });
  bluetoothDevice.addEventListener('gattserverdisconnected', onDisconnected);
}


// This function handles the event 'gattserverdisconnected'
async function onDisconnected() {
  log('> Bluetooth Device disconnected');
  try {
    await connectDeviceAndCacheCharacteristics();
    log('> Reconnected.');
  } catch (error) {
    log('> Reconnection failed. Please connect manually.');
  }
}


// This function connects the web browser to the BLE device and
// gets the Services and their corresponding Characteristics.
async function connectDeviceAndCacheCharacteristics() {
  if (bluetoothDevice.gatt.connected &&
      wifiScannerStateCharacteristic &&
      wifiConfigStateCharacteristic) {
    return;
  }

  // Retry gatt.connect() — Chrome/BlueZ may need a moment after a disconnect
  // before a new connection succeeds. This covers both initial connect after a
  // page refresh and reconnect after the device drops the link.
  let server;
  for (let attempt = 1; attempt <= MAX_RECONNECT_ATTEMPTS; attempt++) {
    if (attempt > 1) {
      log(`> Retrying connection (attempt ${attempt}/${MAX_RECONNECT_ATTEMPTS})...`);
      await new Promise(resolve => setTimeout(resolve, RECONNECT_DELAY_MS));
    }
    log('> Connecting to GATT Server...');
    try {
      server = await bluetoothDevice.gatt.connect();
      break;
    } catch (error) {
      log('> Error: ' + error);
      if (attempt === MAX_RECONNECT_ATTEMPTS) throw error;
    }
  }

  log('> Getting the Wi-Fi Scanner Service...');
  const wifiScannerService = await server.getPrimaryService(SVC_WIFI_SCANNER_UUID);

  log('> Getting the Wi-Fi Scanner Characteristics...');
  wifiScannerStateCharacteristic = await wifiScannerService.getCharacteristic(CHR_WIFI_SCANNER_STATE_UUID);
  wifiScannerStateCharacteristic.addEventListener('characteristicvaluechanged',
      handleWiFiScannerStateChanged);

  wifiScannerAP_Result_Characteristic = await wifiScannerService.getCharacteristic(CHR_WIFI_SCANNER_RESULT_UUID);

  wifiScannerAP_Select_Characteristic = await wifiScannerService.getCharacteristic(CHR_WIFI_SCANNER_SELECT_UUID);

  log('> Getting the Wi-Fi Configurator Service...');
  const wifiConfigService = await server.getPrimaryService(SVC_WIFI_CONFIG_UUID);

  log('> Getting the Wi-Fi Configurator Characteristics...');
  wifiConfigStateCharacteristic = await wifiConfigService.getCharacteristic(CHR_WIFI_CONFIG_STATE_UUID);
  wifiConfigStateCharacteristic.addEventListener('characteristicvaluechanged',
      handleWiFiConfigStateChanged);

  wifiConfigSSIDCharacteristic = await wifiConfigService.getCharacteristic(CHR_WIFI_CONFIG_SSID_UUID);
  wifiConfigPskCharacteristic = await wifiConfigService.getCharacteristic(CHR_WIFI_CONFIG_PASSWORD_UUID);

  const wifiAuthService = await server.getPrimaryService(SVC_WIFI_AUTH_UUID);
  wifiAuthKeyCharacteristic = await wifiAuthService.getCharacteristic(CHR_WIFI_AUTH_KEY_UUID);
  var hash = sha3_256(bleSecret);
  console.log(hash);
  var hash_ab = new Uint8Array(hash.match(/[\da-f]{2}/gi).map(function (value) {
		return parseInt(value, 16)
  }))
  await wifiAuthKeyCharacteristic.writeValue(hash_ab);

  log('> Getting the Network Management Service...');
  const netMgmtService = await server.getPrimaryService(SVC_NET_MGMT_UUID);

  log('> Getting the Network Management Characteristics...');
  netListStatusCharacteristic = await netMgmtService.getCharacteristic(CHR_NET_LIST_STATUS_UUID);
  netListStatusCharacteristic.addEventListener('characteristicvaluechanged',
      handleNetListStateChanged);
  netListSelectCharacteristic = await netMgmtService.getCharacteristic(CHR_NET_LIST_SELECT_UUID);
  netListResultCharacteristic = await netMgmtService.getCharacteristic(CHR_NET_LIST_RESULT_UUID);
  netForgetCharacteristic = await netMgmtService.getCharacteristic(CHR_NET_FORGET_UUID);

  document.querySelector('#btnStatus').disabled = false;
  document.querySelector('#btnListSaved').disabled = false;
}


// This function will be called when 'readValue' resolves and the
// characteristic value changes since 'characteristicvaluechanged' event
// listener has been added. 
function handleWiFiScannerStateChanged(event) {
  let wifiScannerState = event.target.value.getUint8(0);
  log('> Wi-Fi Scanner State is ' + wifiScannerState);

  switch (wifiScannerState) {
    case WIFI_SCANNER_STATE_IDLE:
      document.querySelector('#txtSecret').disabled = true;
      document.querySelector('#btnConnect').disabled = true;
      document.querySelector('#btnScan').disabled = false;
      document.querySelector('#btnReset').disabled = false;
      break;

    case WIFI_SCANNER_STATE_SCANNED:
      readWiFiScannerResults();
      document.querySelector('#txtSecret').disabled = true;
      document.querySelector('#btnConnect').disabled = true;
      document.querySelector('#btnScan').disabled = false;
      document.querySelector('#btnReset').disabled = false;
      document.querySelector('#btnSend').disabled = false;
      document.querySelector('#selAccessPoint').disabled = false;
      document.querySelector('#txtPassword').disabled = false;
      break;

    case WIFI_SCANNER_STATE_ERROR:
      document.querySelector('#btnConnect').disabled = true;
      document.querySelector('#btnScan').disabled = false;
      document.querySelector('#btnReset').disabled = false;
      break;
  }
}


// This function will be called when 'readValue' resolves and the
// characteristic value changes since 'characteristicvaluechanged' event
// listener has been added. 
function handleWiFiConfigStateChanged(event) {
  let wifiConfigState = event.target.value.getUint8(0);
  log('> Wi-Fi Config State is ' + wifiConfigState);

  switch (wifiConfigState) {
    case WIFI_CONFIG_STATE_IDLE:
      document.querySelector('#btnConnect').disabled = true;
      document.querySelector('#btnScan').disabled = false;
      document.querySelector('#btnReset').disabled = false;
      break;

    case WIFI_CONFIG_STATE_CONNECT:
      document.querySelector('#txtSecret').disabled = true;
      document.querySelector('#btnConnect').disabled = true;
      document.querySelector('#btnScan').disabled = false;
      document.querySelector('#btnReset').disabled = false;
      document.querySelector('#btnSend').disabled = false;
      document.querySelector('#selAccessPoint').disabled = false;
      document.querySelector('#txtPassword').disabled = false;
      break;

    case WIFI_CONFIG_STATE_JOINED:
      joinedEventHandler();
      document.querySelector('#txtSecret').disabled = true;
      document.querySelector('#btnConnect').disabled = true;
      document.querySelector('#btnScan').disabled = false;
      document.querySelector('#btnReset').disabled = false;
      document.querySelector('#btnSend').disabled = false;
      document.querySelector('#selAccessPoint').disabled = false;
      document.querySelector('#txtPassword').disabled = false;
      break;

    case WIFI_CONFIG_STATE_ERROR:
      document.querySelector('#txtSecret').disabled = true;
      document.querySelector('#btnConnect').disabled = true;
      document.querySelector('#btnScan').disabled = false;
      document.querySelector('#btnReset').disabled = false;
      document.querySelector('#btnSend').disabled = false;
      document.querySelector('#selAccessPoint').disabled = false;
      document.querySelector('#txtPassword').disabled = false;
      break;
  }
}

// This function reads the Wi-Fi scanner results,
// prevents any further notifications and resets 
// the scanner service to the idle state. 
async function readWiFiScannerResults() {
  try {

    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

	result_all = "";
    log('> Reading Wi-Fi Scanner Results...');
	value = await wifiScannerAP_Select_Characteristic.readValue();
	max_records = value.getUint8(0);
	console.log(`Number of result records: ${max_records}`);
	var enc = new TextDecoder("utf-8");
	for (let i = 0; i < max_records; i++) {
		const select_value = Uint8Array.of(i);
		await wifiScannerAP_Select_Characteristic.writeValue(select_value)
		result_part = await wifiScannerAP_Result_Characteristic.readValue();
		str = enc.decode(new Uint8Array(result_part.buffer));
		console.log(`Result part ${str}`);
		result_all += str;
	}

	log('> Results: ' + result_all);
      
    try {
      var obj = JSON.parse(result_all);
      log('> Results: ' + JSON.stringify(obj, undefined, 2));
      if (obj.length > 0) {
        obj.sort((a, b) => (Number(a.rssi) < Number(b.rssi)) ? 1 : -1);
        // Deduplicate by SSID: multiple BSSIDs share an SSID in mesh/multi-AP
        // networks. After sorting by RSSI, the first entry per SSID is the
        // strongest; discard the rest.
        const seen = new Set();
        const unique = obj.filter(ap => {
          if (seen.has(ap.ssid)) return false;
          seen.add(ap.ssid);
          return true;
        });
        var x = document.getElementById("selAccessPoint");
        while (x.firstChild) {
          x.removeChild(x.firstChild);
        }
        for (i = 0; i < unique.length; i++) {
          var option = document.createElement("option");
          option.text = unique[i].ssid;
          option.value = unique[i].ssid;
          x.add(option);
        }
      }
    } catch (e) {
      log('> Error: ' + e.name + ': ' + e.message);
    }
	
    log('> Stop Wi-Fi Scanner State Notifications...');
    await wifiScannerStateCharacteristic.stopNotifications();

    // Reset the Wi-Fi scanner state back to idle
    var wifiScannerState = Uint8Array.of(WIFI_SCANNER_STATE_IDLE);
    await wifiScannerStateCharacteristic.writeValue(wifiScannerState);

    // Read the Wi-Fi scanner state to confirm
    await wifiScannerStateCharacteristic.readValue();

  } catch (error) {
    log('> Error: ' + error);
  }
}

// This function will be called when the net-list-status characteristic notifies.
function handleNetListStateChanged(event) {
  let state = event.target.value.getUint8(0);
  log('> Net List State is ' + state);
}


// This function triggers a refresh of the saved-network list, waits for the
// notify to signal completion, reads all chunks, and renders the result.
async function listSavedNetworks() {
  try {
    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

    document.querySelector('#btnListSaved').disabled = true;
    document.querySelector('#savedNetworksList').innerHTML = '';

    log('> Starting Net List Status Notifications...');
    await netListStatusCharacteristic.startNotifications();

    // Write 1 (Loading) to trigger a refresh on the device
    await netListStatusCharacteristic.writeValue(Uint8Array.of(NET_LIST_STATE_LOADING));

    // Poll until the device signals Finished or Error
    let state = NET_LIST_STATE_LOADING;
    while (state === NET_LIST_STATE_LOADING) {
      await new Promise(resolve => setTimeout(resolve, 500));
      const v = await netListStatusCharacteristic.readValue();
      state = v.getUint8(0);
    }

    await netListStatusCharacteristic.stopNotifications();

    if (state !== NET_LIST_STATE_FINISHED) {
      log('> Net list refresh failed (state=' + state + ')');
      document.querySelector('#btnListSaved').disabled = false;
      return;
    }

    log('> Reading saved network list...');
    const countValue = await netListSelectCharacteristic.readValue();
    const chunkCount = countValue.getUint8(0);
    console.log(`Saved network chunk count: ${chunkCount}`);

    const enc = new TextDecoder('utf-8');
    let json = '';
    for (let i = 0; i < chunkCount; i++) {
      await netListSelectCharacteristic.writeValue(Uint8Array.of(i));
      const chunk = await netListResultCharacteristic.readValue();
      json += enc.decode(new Uint8Array(chunk.buffer));
    }

    log('> Saved networks raw: ' + json);

    try {
      const networks = JSON.parse(json);
      renderSavedNetworks(networks);
    } catch (e) {
      log('> Error parsing saved networks: ' + e.message);
    }

    // Reset the characteristic back to idle
    await netListStatusCharacteristic.writeValue(Uint8Array.of(NET_LIST_STATE_IDLE));

    document.querySelector('#btnListSaved').disabled = false;
  } catch (error) {
    log('> Error: ' + error);
    document.querySelector('#btnListSaved').disabled = false;
  }
}


// This function renders the list of saved networks in the UI.
function renderSavedNetworks(networks) {
  const list = document.querySelector('#savedNetworksList');
  list.innerHTML = '';
  if (!networks || networks.length === 0) {
    list.innerHTML = '<li><em>No saved networks</em></li>';
    return;
  }
  networks.forEach(function(net) {
    const li = document.createElement('li');
    const label = net.flags ? net.ssid + ' ' + net.flags : net.ssid;
    li.textContent = label + ' ';
    const btn = document.createElement('button');
    btn.textContent = 'Forget';
    btn.onclick = function() { onForgetButtonClick(net.ssid); };
    li.appendChild(btn);
    list.appendChild(li);
  });
}


// This function forgets a saved network by SSID.
async function forgetNetwork(ssid) {
  try {
    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

    log('> Forgetting network: ' + ssid);
    const enc = new TextEncoder();
    await netForgetCharacteristic.writeValue(enc.encode(ssid));
    log('> Network forgotten: ' + ssid);

    // Refresh the saved list to reflect the removal
    await listSavedNetworks();
  } catch (error) {
    log('> Error: ' + error);
  }
}


// This function handles the click event of a 'Forget' button next to a saved network.
async function onForgetButtonClick(ssid) {
  await forgetNetwork(ssid);
}


// This function handles the click event of the 'List Saved Networks' button.
async function onListSavedButtonClick() {
  await listSavedNetworks();
}


async function joinedEventHandler() {
  log('> Connected to AP.');
  log('> Stop Wi-Fi Config State Notifications...');
  await wifiConfigStateCharacteristic.stopNotifications();
  document.querySelector('#btnDisconnect').disabled = false;
}


// This function handles the click event of the button 'Disconnect'.
async function onDisconnectButtonClick() {
  try {
    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

    log('> Disconnecting from AP...');
    var wifiConfigState = Uint8Array.of(WIFI_CONFIG_STATE_IDLE);
    await wifiConfigStateCharacteristic.writeValue(wifiConfigState);
    document.querySelector('#btnDisconnect').disabled = true;
  } catch (error) {
    log('> Error: ' + error);
  }
}


// This function handles the click event of the button 'Get Status'.
// Note: IP address is not exposed over BLE; only state and SSID are available.
async function onStatusButtonClick() {
  try {
    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

    log('> Reading connection status...');

    const stateValue = await wifiConfigStateCharacteristic.readValue();
    const state = stateValue.getUint8(0);
    const stateLabels = ['idle', 'connecting', 'connected', 'error'];
    log('> State: ' + (stateLabels[state] ?? state));

    const ssidValue = await wifiConfigSSIDCharacteristic.readValue();
    const ssid = new TextDecoder('utf-8').decode(new Uint8Array(ssidValue.buffer));
    if (ssid.length > 0) {
      log('> SSID: ' + ssid);
    }
  } catch (error) {
    log('> Error: ' + error);
  }
}


// This function clears the list of Access Points
function removeAllAccessPoints() {
  var x = document.getElementById("selAccessPoint");
  while (x.firstChild) {
    x.removeChild(x.firstChild);
  }
  accessPointsObj = [];
}


// This function handles the click event of the button 'Connect'.
async function onConnectButtonClick() {
  try {
    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

    log('> Reading Wi-Fi Scanner State...');
    await wifiScannerStateCharacteristic.readValue();
  } catch (error) {
    log('> Error: ' + error);
  }
}


// This function handles the click event of the button 'Reset Device'.
function onResetButtonClick() {
  // Disable/Enable the buttons
  document.querySelector('#txtSecret').disabled = false;
  document.querySelector('#btnConnect').disabled = false;
  document.querySelector('#btnScan').disabled = true;
  document.querySelector('#btnReset').disabled = true;
  document.querySelector('#btnSend').disabled = true;
  document.querySelector('#btnStatus').disabled = true;
  document.querySelector('#btnDisconnect').disabled = true;
  document.querySelector('#selAccessPoint').disabled = true;
  document.querySelector('#txtPassword').disabled = true;

  removeAllAccessPoints();

  if (wifiScannerStateCharacteristic) {
    wifiScannerStateCharacteristic.removeEventListener('characteristicvaluechanged',
        handleWiFiScannerStateChanged);
        wifiScannerStateCharacteristic = null;
  }
  if (netListStatusCharacteristic) {
    netListStatusCharacteristic.removeEventListener('characteristicvaluechanged',
        handleNetListStateChanged);
    netListStatusCharacteristic = null;
  }
  netListSelectCharacteristic = null;
  netListResultCharacteristic = null;
  netForgetCharacteristic = null;
  document.querySelector('#savedNetworksList').innerHTML = '';
  // Note that it doesn't disconnect device.
  bluetoothDevice = null;
  log('> Bluetooth Device reset');
}


// This function handles the click event of the button 'Start Scan'.
async function onScanButtonClick() {
  try {
    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

    log('> Starting a Wi-Fi Scan...');

    document.querySelector('#btnScan').disabled = true;
    document.querySelector('#btnSend').disabled = true;
    removeAllAccessPoints();
    document.querySelector('#selAccessPoint').disabled = true;
    document.querySelector('#txtPassword').disabled = true;

    log('> Starting Wi-Fi Scanner State Notifications...');
    await wifiScannerStateCharacteristic.startNotifications();

    log('> Writing Wi-Fi Scanner State...');
    var wifiScannerState = Uint8Array.of(WIFI_SCANNER_STATE_SCAN);
    await wifiScannerStateCharacteristic.writeValue(wifiScannerState);

  } catch (error) {
    log('> Error: ' + error);
  }
}

async function onPskGenerated(psk)
{
  try {
	log('> Sending SSID and PSK...');

	var ssid = document.querySelector('#selAccessPoint').value;
	var psk_ab = new Uint8Array(psk.match(/[\da-f]{2}/gi).map(function (value) {
		return parseInt(value, 16)
	}))
	var enc = new TextEncoder();
	var ssid_ab = enc.encode(ssid);
	
	await wifiConfigSSIDCharacteristic.writeValue(ssid_ab.buffer);
	
	await wifiConfigPskCharacteristic.writeValue(psk_ab.buffer);

    log('> Starting Wi-Fi Config State Notifications...');
    await wifiConfigStateCharacteristic.startNotifications();

    log('> Writing Wi-Fi Config State...');
    var wifiConfigState = Uint8Array.of(WIFI_CONFIG_STATE_CONNECT);
    await wifiConfigStateCharacteristic.writeValue(wifiConfigState);
  } catch (error) {
    log('> Error: ' + error);
  }
}

// This function handles the click event of the button 'Save Access Point'.
async function onSendButtonClick() {
  try {
    if (!bluetoothDevice) {
      await requestDevice();
    }
    await connectDeviceAndCacheCharacteristics();

    document.querySelector('#btnScan').disabled = true;
    document.querySelector('#btnSend').disabled = true;
    document.querySelector('#selAccessPoint').disabled = true;
    document.querySelector('#txtPassword').disabled = true;

	var passphrase = document.querySelector('#txtPassword').value;
	var ssid = document.querySelector('#selAccessPoint').value;

    log('> Generating PSK for ' + ssid + " and " + passphrase);

	// Sanity checks
	if (!passphrase || !ssid)
		return log('> Please select AP and specify passphrase');

	var psk = "invalid";
	var pskgen = new PBKDF2(passphrase, ssid, 4096, 256 / 8);
	var progress = function(percent_done) { };
	pskgen.deriveKey(progress, onPskGenerated);
  } catch (error) {
    log('> Error: ' + error);
  }
}

