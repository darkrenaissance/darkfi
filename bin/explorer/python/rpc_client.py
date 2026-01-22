import asyncio
import json
from dataclasses import dataclass
from typing import Any
from contextlib import asynccontextmanager


class JsonRpcError(Exception):
    def __init__(self, code: int, message: str, data: Any = None):
        self.code = code
        self.message = message
        self.data = data
        super().__init__(f"RPC Error {code}: {message}")


class JsonRpcConnection:
    """Single JSON-RPC connection over TCP"""

    def __init__(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        self.reader = reader
        self.writer = writer
        self.request_id = 0
        self._lock = asyncio.Lock()
        self._closed = False

    @property
    def is_closed(self) -> bool:
        return self._closed or self.writer.is_closing()

    async def call(self, method: str, params: Any = None, timeout: float = 30.0) -> Any:
        if self.is_closed:
            raise ConnectionError("Connection is closed")

        async with self._lock:
            self.request_id += 1
            request = {
                "jsonrpc": "2.0",
                "method": method,
                "id": self.request_id,
                "params": params,
            }

            message = json.dumps(request) + "\n"

            try:
                self.writer.write(message.encode("utf-8"))
                await self.writer.drain()

                response_line = await asyncio.wait_for(
                    self.reader.readline(),
                    timeout=timeout
                )
            except asyncio.TimeoutError:
                self._closed = True
                raise TimeoutError(f"RPC call '{method}' timed out after {timeout}s")
            except (ConnectionError, OSError) as e:
                self._closed = True
                raise ConnectionError(f"Connection lost: {e}")

            if not response_line:
                self._closed = True
                raise ConnectionError("Connection closed by server")

            response = json.loads(response_line.decode("utf-8"))

            if "error" in response and response["error"]:
                err = response["error"]
                raise JsonRpcError(
                    err.get("code", -1),
                    err.get("message", "Unknown error"),
                    err.get("data")
                )

            return response.get("result")

    async def close(self):
        if not self._closed:
            self._closed = True
            self.writer.close()
            try:
                await self.writer.wait_closed()
            except Exception:
                pass


class JsonRpcPool:
    """Connection pool with automatic reconnection"""

    def __init__(
        self,
        host: str,
        port: int,
        min_connections: int = 5,
        max_connections: int = 20,
    ):
        self.host = host
        self.port = port
        self.min_connections = min_connections
        self.max_connections = max_connections

        self._pool: asyncio.Queue[JsonRpcConnection] = None
        self._semaphore: asyncio.Semaphore = None
        self._connection_count = 0
        self._lock = asyncio.Lock()
        self._closed = False

    async def start(self):
        """Initialize the pool with minimum connections"""
        self._pool = asyncio.Queue()
        self._semaphore = asyncio.Semaphore(self.max_connections)
        self._connection_count = 0

        for _ in range(self.min_connections):
            try:
                conn = await self._create_connection()
                await self._pool.put(conn)
            except Exception as e:
                print(f"Warning: Failed to create initial connection: {e}")

    async def _create_connection(self) -> JsonRpcConnection:
        reader, writer = await asyncio.open_connection(self.host, self.port)
        async with self._lock:
            self._connection_count += 1
        return JsonRpcConnection(reader, writer)

    async def _destroy_connection(self, conn: JsonRpcConnection):
        await conn.close()
        async with self._lock:
            self._connection_count -= 1

    @asynccontextmanager
    async def connection(self):
        """Acquire a connection from the pool"""
        if self._closed:
            raise RuntimeError("Pool's closed")

        conn = None

        async with self._semaphore:
            # Try to get an existing connection
            while not self._pool.empty():
                conn = await self._pool.get()
                if not conn.is_closed:
                    break
                await self._destroy_connection(conn)
                conn = None

            # Create new if needed
            if conn is None:
                conn = await self._create_connection()

            try:
                yield conn
            except (ConnectionError, TimeoutError):
                # Connection is bad, don't return to pool
                await self._destroy_connection(conn)
                raise
            else:
                # Return healthy connection to pool
                if not conn.is_closed:
                    await self._pool.put(conn)
                else:
                    await self._destroy_connection(conn)

    async def call(self, method: str, params: Any = None, timeout: float = 30.0) -> Any:
        """Make an RPC call using a pooled connection"""
        async with self.connection() as conn:
            return await conn.call(method, params, timeout)

    async def close(self):
        """Close all connections"""
        self._closed = True
        while not self._pool.empty():
            conn = await self._pool.get()
            await self._destroy_connection(conn)
