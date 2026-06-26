#!/usr/bin/env python3
"""Minimal mock ttyd for integration tests.

Listens on the Unix socket given as argv[1], responds to HTTP requests with the
received method/path/headers, and echoes WebSocket messages.
"""
import asyncio
import json
import os
import signal
import sys
from http import HTTPStatus

SOCKET = sys.argv[1]


async def http_handler(reader, writer):
    request_line = await reader.readline()
    if not request_line:
        return
    parts = request_line.decode().strip().split()
    if len(parts) < 2:
        return
    method, raw_path = parts[0], parts[1]
    is_ws = False
    headers = {}
    while True:
        line = await reader.readline()
        if line in (b"\r\n", b"\n"):
            break
        key, _, value = line.decode().strip().partition(":")
        key = key.strip().lower()
        value = value.strip()
        headers[key] = value
        if key == "upgrade" and "websocket" in value.lower():
            is_ws = True

    # Read body if present
    body_len = int(headers.get("content-length", "0"))
    body = b""
    if body_len:
        body = await reader.read(body_len)

    if is_ws:
        # Very minimal WebSocket handshake echo server.
        ws_accept = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        writer.write(
            b"HTTP/1.1 101 Switching Protocols\r\n"
            + b"Upgrade: websocket\r\n"
            + b"Connection: Upgrade\r\n"
            + f"Sec-WebSocket-Accept: {ws_accept}\r\n".encode()
            + b"\r\n"
        )
        await writer.drain()
        # Echo frames back.
        try:
            while True:
                header = await reader.read(2)
                if len(header) < 2:
                    break
                fin_opcode = header[0]
                payload_len = header[1] & 0x7F
                if payload_len == 126:
                    payload_len = int.from_bytes(await reader.read(2), "big")
                elif payload_len == 127:
                    payload_len = int.from_bytes(await reader.read(8), "big")
                mask = await reader.read(4)
                data = bytearray(await reader.read(payload_len))
                for i in range(len(data)):
                    data[i] ^= mask[i % 4]
                # Send same frame back unmasked.
                out_len = len(data)
                out_header = bytearray()
                out_header.append(fin_opcode)
                if out_len < 126:
                    out_header.append(out_len)
                elif out_len < 65536:
                    out_header.append(126)
                    out_header.extend(out_len.to_bytes(2, "big"))
                else:
                    out_header.append(127)
                    out_header.extend(out_len.to_bytes(8, "big"))
                writer.write(out_header + data)
                await writer.drain()
        except Exception:
            pass
        writer.close()
        return

    response = json.dumps({
        "method": method,
        "path": raw_path,
        "headers": headers,
        "body": body.decode("utf-8", "replace"),
    })
    encoded = response.encode()
    writer.write(
        f"HTTP/1.1 {HTTPStatus.OK.value} OK\r\n".encode()
        + b"Content-Type: application/json\r\n"
        + f"Content-Length: {len(encoded)}\r\n".encode()
        + b"Connection: close\r\n\r\n"
        + encoded
    )
    await writer.drain()
    writer.close()


async def main():
    try:
        os.unlink(SOCKET)
    except FileNotFoundError:
        pass
    except OSError:
        pass

    loop = asyncio.get_event_loop()
    for sig in (signal.SIGTERM, signal.SIGINT):
        loop.add_signal_handler(sig, lambda: asyncio.create_task(shutdown()))

    server = await asyncio.start_unix_server(http_handler, SOCKET)
    await asyncio.Future()


async def shutdown():
    tasks = [t for t in asyncio.all_tasks() if t is not asyncio.current_task()]
    for t in tasks:
        t.cancel()
    await asyncio.gather(*tasks, return_exceptions=True)
    asyncio.get_event_loop().stop()


if __name__ == "__main__":
    asyncio.run(main())
