# Ethereum Rust API Documentation

## JSON-RPC API Reference

The Ethereum Rust node provides a complete JSON-RPC API compatible with the Ethereum specification.

### Connection Methods

#### HTTP
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

#### WebSocket
```javascript
const ws = new WebSocket('ws://localhost:8546');
ws.send(JSON.stringify({
    jsonrpc: '2.0',
    method: 'eth_blockNumber',
    params: [],
    id: 1
}));
```

## API Methods

### eth Namespace

#### eth_blockNumber
Returns the current block number.

**Parameters**: None

**Returns**: `QUANTITY` - Current block number

**Example**:
```json
{
    "jsonrpc": "2.0",
    "method": "eth_blockNumber",
    "params": [],
    "id": 1
}
```

#### eth_getBlockByHash
Returns information about a block by hash.

**Parameters**:
1. `DATA`, 32 Bytes - Hash of a block
2. `Boolean` - If true returns full transaction objects, if false only hashes

**Returns**: `Object` - Block object or null

**Example**:
```json
{
    "jsonrpc": "2.0",
    "method": "eth_getBlockByHash",
    "params": [
        "0xb903239f8543d04b5dc1ba6579132b143087c68db1b2168786408fcbce568238",
        true
    ],
    "id": 1
}
```

#### eth_getBlockByNumber
Returns information about a block by block number.

**Parameters**:
1. `QUANTITY|TAG` - Block number or "latest", "earliest", "pending"
2. `Boolean` - If true returns full transaction objects

**Returns**: `Object` - Block object or null

#### eth_getTransactionByHash
Returns information about a transaction by hash.

**Parameters**:
1. `DATA`, 32 Bytes - Hash of a transaction

**Returns**: `Object` - Transaction object or null

#### eth_sendRawTransaction
Submits a raw transaction.

**Parameters**:
1. `DATA` - Signed transaction data

**Returns**: `DATA`, 32 Bytes - Transaction hash

**Example**:
```json
{
    "jsonrpc": "2.0",
    "method": "eth_sendRawTransaction",
    "params": ["0xf869018203e882..."],
    "id": 1
}
```

#### eth_call
Executes a new message call immediately without creating a transaction.

**Parameters**:
1. `Object` - Transaction call object
   - `from`: `DATA`, 20 Bytes - (optional) Address call is from
   - `to`: `DATA`, 20 Bytes - Address call is directed to
   - `gas`: `QUANTITY` - (optional) Gas for execution
   - `gasPrice`: `QUANTITY` - (optional) Gas price
   - `value`: `QUANTITY` - (optional) Value sent
   - `data`: `DATA` - (optional) Method signature and encoded parameters
2. `QUANTITY|TAG` - Block number or "latest", "earliest", "pending"

**Returns**: `DATA` - Return value of executed contract

#### eth_estimateGas
Estimates gas needed for transaction execution.

**Parameters**:
1. `Object` - Transaction object (same as eth_call)

**Returns**: `QUANTITY` - Gas amount

#### eth_getBalance
Returns the balance of an account.

**Parameters**:
1. `DATA`, 20 Bytes - Address to check balance
2. `QUANTITY|TAG` - Block number or tag

**Returns**: `QUANTITY` - Current balance in wei

#### eth_getCode
Returns code at a given address.

**Parameters**:
1. `DATA`, 20 Bytes - Address
2. `QUANTITY|TAG` - Block number or tag

**Returns**: `DATA` - Code from the given address

#### eth_getStorageAt
Returns the value from a storage position.

**Parameters**:
1. `DATA`, 20 Bytes - Storage address
2. `QUANTITY` - Position in storage
3. `QUANTITY|TAG` - Block number or tag

**Returns**: `DATA` - Value at storage position

#### eth_getTransactionCount
Returns the number of transactions sent from an address.

**Parameters**:
1. `DATA`, 20 Bytes - Address
2. `QUANTITY|TAG` - Block number or tag

**Returns**: `QUANTITY` - Transaction count (nonce)

#### eth_getTransactionReceipt
Returns the receipt of a transaction.

**Parameters**:
1. `DATA`, 32 Bytes - Transaction hash

**Returns**: `Object` - Transaction receipt or null

#### eth_getLogs
Returns logs matching filter criteria.

**Parameters**:
1. `Object` - Filter object
   - `fromBlock`: `QUANTITY|TAG` - (optional) From block
   - `toBlock`: `QUANTITY|TAG` - (optional) To block
   - `address`: `DATA|Array` - (optional) Contract address(es)
   - `topics`: `Array of DATA` - (optional) Topic filters
   - `blockhash`: `DATA`, 32 Bytes - (optional) Restrict to single block

**Returns**: `Array` - Array of log objects

#### eth_syncing
Returns sync status or false.

**Returns**: `Object|Boolean` - Sync status or false if not syncing

#### eth_gasPrice
Returns current gas price.

**Returns**: `QUANTITY` - Current gas price in wei

### net Namespace

#### net_version
Returns the network ID.

**Returns**: `String` - Current network ID

#### net_listening
Returns true if client is listening for connections.

**Returns**: `Boolean` - true if listening

#### net_peerCount
Returns number of connected peers.

**Returns**: `QUANTITY` - Number of connected peers

### web3 Namespace

#### web3_clientVersion
Returns the client version.

**Returns**: `String` - Client version string

#### web3_sha3
Returns Keccak-256 hash of data.

**Parameters**:
1. `DATA` - Data to hash

**Returns**: `DATA`, 32 Bytes - Keccak-256 hash

### debug Namespace

#### debug_traceTransaction
Returns execution trace of transaction.

**Parameters**:
1. `DATA`, 32 Bytes - Transaction hash
2. `Object` - (optional) Trace options

**Returns**: `Object` - Execution trace

#### debug_traceBlockByNumber
Returns execution traces for all transactions in block.

**Parameters**:
1. `QUANTITY|TAG` - Block number
2. `Object` - (optional) Trace options

**Returns**: `Array` - Array of traces

### trace Namespace

#### trace_transaction
Returns trace of a transaction.

**Parameters**:
1. `DATA`, 32 Bytes - Transaction hash

**Returns**: `Array` - Array of traces

#### trace_block
Returns traces for all transactions in block.

**Parameters**:
1. `QUANTITY|TAG` - Block number

**Returns**: `Array` - Array of traces

#### trace_filter
Returns traces matching filter.

**Parameters**:
1. `Object` - Filter object
   - `fromBlock`: `QUANTITY|TAG` - (optional)
   - `toBlock`: `QUANTITY|TAG` - (optional)
   - `fromAddress`: `Array of DATA` - (optional)
   - `toAddress`: `Array of DATA` - (optional)
   - `after`: `QUANTITY` - (optional) Offset
   - `count`: `QUANTITY` - (optional) Number of traces

**Returns**: `Array` - Array of traces

## WebSocket Subscriptions

### eth_subscribe
Creates a subscription for real-time events.

**Parameters**:
1. `String` - Subscription type
   - "newHeads" - New block headers
   - "logs" - Log entries
   - "pendingTransactions" - Pending transactions
   - "syncing" - Sync status changes
2. `Object` - (optional) Filter options for logs

**Returns**: `QUANTITY` - Subscription ID

**Example**:
```json
{
    "jsonrpc": "2.0",
    "method": "eth_subscribe",
    "params": ["newHeads"],
    "id": 1
}
```

### eth_unsubscribe
Cancels a subscription.

**Parameters**:
1. `QUANTITY` - Subscription ID

**Returns**: `Boolean` - true if cancelled successfully

## Error Codes

Standard JSON-RPC error codes:

- `-32700`: Parse error
- `-32600`: Invalid request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error
- `-32000`: Server error

Ethereum-specific error codes:

- `3`: Execution error
- `2`: Action not allowed
- `1`: Resource not found

## Rate Limiting

Default rate limits:
- HTTP: 100 requests/second
- WebSocket: 200 messages/second
- Batch requests: Maximum 100 calls per batch

## Authentication

JWT authentication is supported for secure deployments:

```bash
# Generate JWT secret
openssl rand -hex 32 > jwt.secret

# Start node with JWT
ethereum-rust --rpc-jwt-secret jwt.secret

# Make authenticated request
curl -H "Authorization: Bearer <jwt-token>" \
     -X POST http://localhost:8545 \
     -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

## CORS Configuration

Configure CORS in `config.toml`:

```toml
[rpc]
cors_origins = ["http://localhost:3000", "https://myapp.com"]
cors_headers = ["Content-Type", "Authorization"]
```

## Examples

### Get Account Balance
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "eth_getBalance",
    "params": ["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"],
    "id": 1
  }'
```

### Send Transaction
```javascript
const tx = {
    from: "0x407d73d8a49eeb85d32cf465507dd71d507100c1",
    to: "0x85f43d8a49eeb85d32cf465507dd71d507100c1",
    value: "0x9184e72a000", // 10000000000000 wei
    gas: "0x5208", // 21000
    gasPrice: "0x4a817c800", // 20000000000 wei
};

const response = await fetch('http://localhost:8545', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
        jsonrpc: '2.0',
        method: 'eth_sendTransaction',
        params: [tx],
        id: 1
    })
});
```

### Subscribe to New Blocks
```javascript
const ws = new WebSocket('ws://localhost:8546');

ws.on('open', () => {
    ws.send(JSON.stringify({
        jsonrpc: '2.0',
        method: 'eth_subscribe',
        params: ['newHeads'],
        id: 1
    }));
});

ws.on('message', (data) => {
    const response = JSON.parse(data);
    if (response.method === 'eth_subscription') {
        console.log('New block:', response.params.result);
    }
});
```