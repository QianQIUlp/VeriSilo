"""Minimal HTTP CONNECT test proxy for the transport-fingerprint diagnostic.

This is a test control-plane component, not a fingerprint source. It records
every CONNECT/absolute-URI target (hostname or IP as given by the caller),
resolves hostnames through the OS resolver, logs the resolution, and then
tunnels raw bytes so the TLS session between browser and observation server
remains end-to-end (no TLS termination).
"""

import json
import socket
import threading
import time
from pathlib import Path

RUNTIME = Path(__file__).resolve().parent.parent / "runtime"
LOG_PATH = RUNTIME / "proxy-observations.jsonl"
LOCK = threading.Lock()


def log(event: dict) -> None:
    event = {"ts": time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime()) + "Z", **event}
    with LOCK:
        with open(LOG_PATH, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(event, ensure_ascii=False) + "\n")


def pump(src: socket.socket, dst: socket.socket) -> None:
    try:
        while True:
            data = src.recv(65535)
            if not data:
                break
            dst.sendall(data)
    except OSError:
        pass
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass


def resolve(host: str, port: int) -> str:
    # This machine's system DNS answers with Clash fake-ip addresses
    # (198.18.0.0/8). The test proxy must reach the local TLS observer, so
    # loopback-mapped diagnostic names are pinned to 127.0.0.1 and fake-ip
    # answers are logged and rejected.
    if host == "localtest.me":
        return "127.0.0.1"
    infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    address = infos[0][4][0]
    if address.startswith("198.18."):
        raise OSError(f"system DNS returned Clash fake-ip {address} for {host}")
    return address


def tunnel(client: socket.socket, host: str, port: int) -> None:
    try:
        resolved = resolve(host, port)
        log({"event": "upstream_resolution", "host": host, "resolved": resolved})
        upstream = socket.create_connection((resolved, port), timeout=10)
        log({"event": "tunnel_established", "host": host, "port": port, "resolved": resolved})
        client.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
        threads = [
            threading.Thread(target=pump, args=(client, upstream), daemon=True),
            threading.Thread(target=pump, args=(upstream, client), daemon=True),
        ]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(300)
    except OSError as error:
        log({"event": "tunnel_failed", "host": host, "port": port, "error": repr(error)})
        try:
            client.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
        except OSError:
            pass
    finally:
        try:
            client.close()
        except OSError:
            pass


def handle(client: socket.socket, addr) -> None:
    try:
        client.settimeout(30)
        head = b""
        while b"\r\n\r\n" not in head and len(head) < 32768:
            chunk = client.recv(32768)
            if not chunk:
                return
            head += chunk
        head_part = head.split(b"\r\n\r\n", 1)[0].decode("latin-1", "replace")
        lines = head_part.split("\r\n")
        request_line = lines[0]
        log({"event": "request", "peer_port": addr[1], "request_line": request_line})
        parts = request_line.split()
        if len(parts) >= 2 and parts[0] == "CONNECT":
            host, _, port = parts[1].rpartition(":")
            tunnel(client, host, int(port))
        elif len(parts) >= 2 and parts[1].startswith("http://"):
            rest = parts[1][7:]
            authority, _, path = rest.partition("/")
            host, _, port = authority.rpartition(":")
            client.sendall(b"HTTP/1.1 405 Use CONNECT for TLS targets\r\nContent-Length: 0\r\n\r\n")
            client.close()
            log({"event": "plain_http_unsupported", "host": host, "port": port, "path": path})
        else:
            client.sendall(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
            client.close()
    except OSError:
        try:
            client.close()
        except OSError:
            pass


def main() -> None:
    RUNTIME.mkdir(exist_ok=True)
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 8888))
    listener.listen(32)
    log({"event": "listening", "port": 8888})
    while True:
        conn, addr = listener.accept()
        threading.Thread(target=handle, args=(conn, addr), daemon=True).start()


if __name__ == "__main__":
    main()
