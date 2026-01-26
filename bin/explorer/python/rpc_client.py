import asyncio
import json
import logging
from typing import Any, Optional
from contextlib import asynccontextmanager

logger = logging.getLogger(__name__)


class JsonRpcError(Exception):
    """Error returned by the RPC server."""
    def __init__(self, code: int, message: str, data: Any = None):
        self.code = code
        self.message = message
        self.data = data
        super().__init__(f"RPC Error {code}: {message}")


class RpcUnavailableError(Exception):
    """Raised when RPC endpoint is not reachable."""
    pass


class JsonRpcConnection:
    """Single JSON-RPC connection over TCP."""

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
    """
    Connection pool with background reconnection.

    - If RPC is unavailable, immediately returns error (no waiting)
    - Background task keeps trying to reconnect every N seconds
    - Once connected, requests work again
    """

    def __init__(
        self,
        host: str,
        port: int,
        min_connections: int = 5,
        max_connections: int = 20,
        reconnect_interval: float = 5.0,
        connect_timeout: float = 5.0,
    ):
        self.host = host
        self.port = port
        self.min_connections = min_connections
        self.max_connections = max_connections
        self.reconnect_interval = reconnect_interval
        self.connect_timeout = connect_timeout

        self._pool: asyncio.Queue[JsonRpcConnection] = None
        self._semaphore: asyncio.Semaphore = None
        self._connection_count = 0
        self._lock = asyncio.Lock()
        self._closed = False
        self._available = False
        self._reconnect_task: Optional[asyncio.Task] = None

    async def start(self):
        """Initialize the pool"""
        self._pool = asyncio.Queue()
        self._semaphore = asyncio.Semaphore(self.max_connections)
        self._connection_count = 0
        self._closed = False

        # Try to create initial connections
        success_count = 0
        for _ in range(self.min_connections):
            conn = await self._create_connection()
            if conn:
                await self._pool.put(conn)
                success_count += 1

        if success_count > 0:
            self._available = True
            logger.info(f"RPC pool started with {success_count} connections")
        else:
            self._available = False
            logger.warning(f"RPC {self.host}:{self.port} unavailable, will retry in background")
            self._start_reconnect_task()

    def _start_reconnect_task(self):
        """Start background reconnection task if not already running."""
        if self._reconnect_task is None or self._reconnect_task.done():
            self._reconnect_task = asyncio.create_task(self._reconnect_loop())

    async def _reconnect_loop(self):
        """Background task that keeps trying to reconnect."""
        while not self._closed and not self._available:
            await asyncio.sleep(self.reconnect_interval)

            if self._closed:
                break

            conn = await self._create_connection()
            if conn:
                await self._pool.put(conn)
                self._available = True
                logger.info(f"RPC {self.host}:{self.port} reconnected")
                break
            else:
                logger.debug(f"RPC {self.host}:{self.port} still unavailable, retrying...")

    async def _create_connection(self) -> Optional[JsonRpcConnection]:
        """Create a new connection. Returns None if connection fails."""
        try:
            reader, writer = await asyncio.wait_for(
                asyncio.open_connection(self.host, self.port),
                timeout=self.connect_timeout
            )
            async with self._lock:
                self._connection_count += 1
            return JsonRpcConnection(reader, writer)
        except (asyncio.TimeoutError, OSError) as e:
            logger.debug(f"Connection failed: {e}")
            return None

    async def _destroy_connection(self, conn: JsonRpcConnection):
        """Close and clean up a connection"""
        await conn.close()
        async with self._lock:
            self._connection_count = max(0, self._connection_count - 1)

    async def call(self, method: str, params: Any = None, timeout: float = 30.0) -> Any:
        """Make an RPC call. Raises RpcUnavailableError immediately if not connected."""
        if self._closed:
            raise RuntimeError("Pool's closed")

        if not self._available:
            raise RpcUnavailableError(f"RPC {self.host}:{self.port} is unavailable")

        async with self._semaphore:
            # Get or create connection
            conn = None
            while not self._pool.empty():
                try:
                    conn = self._pool.get_nowait()
                    if not conn.is_closed:
                        break
                    await self._destroy_connection(conn)
                    conn = None
                except asyncio.QueueEmpty:
                    break

            if conn is None:
                conn = await self._create_connection()
                if conn is None:
                    self._available = False
                    self._start_reconnect_task()
                    raise RpcUnavailableError(f"RPC {self.host}:{self.port} is unavailable")

            # Make the call
            try:
                result = await conn.call(method, params, timeout)
                await self._pool.put(conn)
                return result
            except JsonRpcError:
                # Server error - connection is still good
                await self._pool.put(conn)
                raise
            except (ConnectionError, TimeoutError) as e:
                # Connection failed
                await self._destroy_connection(conn)
                self._available = False
                self._start_reconnect_task()
                raise RpcUnavailableError(f"RPC {self.host}:{self.port} is unavailable: {e}")

    @property
    def is_available(self) -> bool:
        """Check if RPC is currently available."""
        return self._available

    async def close(self):
        """Close all connections"""
        self._closed = True

        if self._reconnect_task and not self._reconnect_task.done():
            self._reconnect_task.cancel()
            try:
                await self._reconnect_task
            except asyncio.CancelledError:
                pass

        while not self._pool.empty():
            try:
                conn = self._pool.get_nowait()
                await self._destroy_connection(conn)
            except asyncio.QueueEmpty:
                break

        logger.info("RPC pool closed")
