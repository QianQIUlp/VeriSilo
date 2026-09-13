"""TLS transport observer for the VeriSilo transport-fingerprint QA diagnostic.

Read-only observation tooling. Terminates TLS on loopback ports, parses the
raw TLS ClientHello emitted by the connecting client (the real managed
browser), then either serves HTTP/1.1 directly or relays the decrypted
plaintext HTTP/2 stream to the local Node h2c logger. Also runs a UDP
listener that records QUIC Initial datagrams (observation only, no response).

All events are appended to runtime/tls-observations.jsonl.
"""

import argparse
import hashlib
import json
import socket
import ssl
import struct
import sys
import threading
import time
import traceback
from pathlib import Path

RUNTIME = Path(__file__).resolve().parent.parent / "runtime"
LOG_PATH = RUNTIME / "tls-observations.jsonl"
HTML_PATH = Path(__file__).resolve().parent / "report_page.html"

LOCK = threading.Lock()

EXTENSION_NAMES = {
    0: "server_name",
    1: "encrypted_client_hello",
    5: "status_request",
    10: "supported_groups",
    11: "ec_point_formats",
    13: "signature_algorithms",
    16: "alpn",
    17: "status_request_v2",
    18: "signed_certificate_timestamp",
    21: "padding",
    22: "encrypt_then_mac",
    23: "extended_master_secret",
    24: "token_binding",
    25: "cached_info",
    26: "tls_lts",
    27: "compress_certificate",
    28: "record_size_limit",
    29: "pwd_protect",
    30: "pwd_clear",
    31: "password_salt",
    32: "ticket_puzzle",
    33: "tls_ticket",
    34: "delegated_credentials",
    35: "session_ticket",
    41: "pre_shared_key",
    42: "early_data",
    43: "supported_versions",
    44: "cookie",
    45: "psk_key_exchange_modes",
    47: "certificate_authorities",
    48: "oid_filters",
    49: "post_handshake_auth",
    50: "signature_algorithms_cert",
    51: "key_share",
    52: "transparency_info",
    53: "connection_id",
    55: "external_id_hash",
    56: "quic_transport_parameters",
    57: "ticket_request",
    13172: "next_protocol_negotiation",
    17513: "application_settings",
    17613: "alps",
    65281: "renegotiation_info",
}

GROUP_NAMES = {
    0x001D: "x25519",
    0x001E: "x448",
    0x0017: "secp256r1",
    0x0018: "secp384r1",
    0x0019: "secp521r1",
    0x001A: "secp192r1",
    0x0100: "ffdhe2048",
    0x0101: "ffdhe3072",
    0x011D: "MLKEM768",
    0x4588: "X25519MLKEM768",
    0x6399: "X25519Kyber768Draft00",
    0x11EC: "X25519MLKEM768Draft06",
}


def is_grease(value: int) -> bool:
    return (value & 0x0F0F) == 0x0A0A


class PeekBuffer:
    """Reads bytes from a socket without consuming them (MSG_PEEK based)."""

    def __init__(self, sock: socket.socket):
        self.sock = sock
        self.buffer = b""

    def ensure(self, count: int, timeout: float = 30.0) -> bytes:
        deadline = time.time() + timeout
        while len(self.buffer) < count:
            remaining = max(0.0, deadline - time.time())
            if remaining == 0:
                raise TimeoutError("timed out waiting for ClientHello bytes")
            self.sock.settimeout(remaining)
            chunk = self.sock.recv(65535, socket.MSG_PEEK)
            if not chunk:
                raise ConnectionError("peer closed before ClientHello completed")
            self.buffer = chunk if len(chunk) > len(self.buffer) else self.buffer
        return self.buffer[:count]

    def all_peeked(self) -> bytes:
        return self.buffer


def parse_client_hello(buf: bytes) -> dict:
    """Parses one full TLS record containing the first ClientHello."""
    record_type, record_major, record_minor, record_len = struct.unpack(">BBBH", buf[:5])
    record = buf[5 : 5 + record_len]
    handshake_type = record[0]
    if handshake_type != 0x01:
        raise ValueError(f"first handshake message is type {handshake_type}, not ClientHello")
    hs_len = int.from_bytes(record[1:4], "big")
    body = record[4 : 4 + hs_len]
    if len(body) < hs_len:
        raise ValueError("fragmented ClientHello across records; not handled")

    offset = 0
    legacy_version = body[offset : offset + 2]
    offset += 2
    random_bytes = body[offset : offset + 32]
    offset += 32
    sid_len = body[offset]
    offset += 1
    session_id = body[offset : offset + sid_len]
    offset += sid_len
    cs_len = int.from_bytes(body[offset : offset + 2], "big")
    offset += 2
    cipher_suites = [struct.unpack(">H", body[i : i + 2])[0] for i in range(offset, offset + cs_len, 2)]
    offset += cs_len
    comp_len = body[offset]
    offset += 1
    compression = list(body[offset : offset + comp_len])
    offset += comp_len

    extensions = []
    extension_order = []
    parsed = {}
    if offset + 2 <= len(body):
        ext_len = int.from_bytes(body[offset : offset + 2], "big")
        offset += 2
        ext_block = body[offset : offset + ext_len]
        i = 0
        while i + 4 <= len(ext_block):
            ext_type = struct.unpack(">H", ext_block[i : i + 2])[0]
            ext_len = int.from_bytes(ext_block[i + 2 : i + 4], "big")
            data = ext_block[i + 4 : i + 4 + ext_len]
            i += 4 + ext_len
            extensions.append(ext_type)
            extension_order.append(EXTENSION_NAMES.get(ext_type, f"ext_{ext_type}"))
            if ext_type == 0 and data:
                sni_len = int.from_bytes(data[3:5], "big")
                parsed["server_name"] = data[5 : 5 + sni_len].decode("utf-8", "replace")
            elif ext_type == 10:
                groups = [struct.unpack(">H", data[j : j + 2])[0] for j in range(2, len(data), 2)]
                parsed["supported_groups"] = [
                    {"value": hex(g), "name": GROUP_NAMES.get(g), "grease": is_grease(g)} for g in groups
                ]
            elif ext_type == 11:
                parsed["ec_point_formats"] = list(data[1:])
            elif ext_type == 13:
                sigalgs = [struct.unpack(">H", data[j : j + 2])[0] for j in range(2, len(data), 2)]
                parsed["signature_algorithms"] = [hex(s) for s in sigalgs]
            elif ext_type == 16:
                i2 = 2
                protos = []
                while i2 < len(data):
                    plen = data[i2]
                    protos.append(data[i2 + 1 : i2 + 1 + plen].decode("utf-8", "replace"))
                    i2 += 1 + plen
                parsed["alpn_offered"] = protos
            elif ext_type == 43:
                versions = [struct.unpack(">H", data[j : j + 2])[0] for j in range(1, len(data), 2)]
                parsed["supported_versions"] = [hex(v) for v in versions]
            elif ext_type == 27:
                parsed["compress_certificate_algorithms"] = [
                    hex(struct.unpack(">H", data[j : j + 2])[0]) for j in range(1, len(data), 2)
                ]
            elif ext_type == 45:
                parsed["psk_key_exchange_modes"] = list(data[1:])
            elif ext_type == 51:
                shares = []
                j = 2
                while j + 4 <= len(data):
                    group = struct.unpack(">H", data[j : j + 2])[0]
                    slen = int.from_bytes(data[j + 2 : j + 4], "big")
                    shares.append({"group": hex(group), "length": slen})
                    j += 4 + slen
                parsed["key_shares"] = shares
            elif ext_type == 21:
                parsed["padding_length"] = len(data)
            elif ext_type == 35:
                parsed["session_ticket"] = True
            elif ext_type == 41:
                parsed["pre_shared_key"] = True

    ja3_version = str(struct.unpack(">H", legacy_version)[0])
    ja3_ciphers = "-".join(str(c) for c in cipher_suites)
    ja3_extensions = "-".join(str(e) for e in extensions)
    ja3_groups = "-".join(
        str(g)
        for entry in parsed.get("supported_groups", [])
        for g in [int(entry["value"], 16)]
    )
    ja3_formats = "-".join(str(f) for f in parsed.get("ec_point_formats", []))
    ja3_string = ",".join([ja3_version, ja3_ciphers, ja3_extensions, ja3_groups, ja3_formats])

    return {
        "record_version": f"{record_major:#04x}{record_minor:02x}",
        "legacy_version": legacy_version.hex(),
        "random_first4": random_bytes[:4].hex(),
        "session_id_length": sid_len,
        "cipher_suites": [hex(c) for c in cipher_suites],
        "cipher_suites_grease": [c for c in cipher_suites if is_grease(c)],
        "compression_methods": compression,
        "extension_order_numeric": extensions,
        "extension_order": extension_order,
        "extensions_parsed": parsed,
        "client_hello_sha256": hashlib.sha256(body).hexdigest(),
        "client_hello_length": hs_len,
        "ja3_string": ja3_string,
        "ja3_md5": hashlib.md5(ja3_string.encode()).hexdigest(),
        "full_client_hello_hex": body.hex(),
    }


def log_event(event: dict) -> None:
    event = {"ts": time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime()) + "Z", **event}
    with LOCK:
        with open(LOG_PATH, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(event, ensure_ascii=False) + "\n")


def load_html() -> bytes:
    return HTML_PATH.read_bytes()


def pump(src: socket.socket, dst: socket.socket, label: str) -> None:
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


def http1_serve(tls_sock: ssl.SSLSocket, conn_info: dict, html: bytes) -> None:
    tls_sock.settimeout(30)
    head = b""
    while b"\r\n\r\n" not in head and len(head) < 65536:
        chunk = tls_sock.recv(65535)
        if not chunk:
            break
        head += chunk
    head_part, _, _ = head.partition(b"\r\n\r\n")
    lines = head_part.decode("latin-1", "replace").split("\r\n")
    request = {
        "event": "http1_request",
        **conn_info,
        "request_line": lines[0] if lines else None,
        "header_order": [line.split(":", 1)[0].strip() for line in lines[1:] if ":" in line],
        "headers": [line for line in lines[1:] if line],
    }
    log_event(request)
    alt_svc = b'Alt-Svc: h3=":8447"; ma=3600\r\n'
    tls_sock.sendall(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n"
        + alt_svc
        + b"Content-Length: " + str(len(html)).encode() + b"\r\nConnection: close\r\n\r\n"
        + html
    )
    try:
        while True:
            extra = tls_sock.recv(65535)
            if not extra:
                break
            body_part = extra.decode("utf-8", "replace")
            if "GET /jsprobe" in body_part:
                log_event({"event": "jsprobe_http1", **conn_info, "line": body_part.splitlines()[0]})
    except OSError:
        pass


def h2_relay(tls_sock: ssl.SSLSocket, conn_info: dict, h2c_port: int) -> None:
    upstream = socket.create_connection(("127.0.0.1", h2c_port), timeout=10)
    threads = [
        threading.Thread(target=pump, args=(tls_sock, upstream, "tls->h2c"), daemon=True),
        threading.Thread(target=pump, args=(upstream, tls_sock, "h2c->tls"), daemon=True),
    ]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join(300)


def handle_connection(conn: socket.socket, addr, port: int, ctx: ssl.SSLContext, h2c_port: int) -> None:
    try:
        peeker = PeekBuffer(conn)
        try:
            header = peeker.ensure(5)
        except (TimeoutError, ConnectionError, OSError) as error:
            log_event({"event": "non_tls_or_closed", "port": port, "peer": addr[0], "error": str(error)})
            return
        record_type = header[0]
        if record_type != 0x16:
            # Not a TLS handshake (e.g. plain HTTP probe). Capture once, politely.
            blob = peeker.ensure(5 + int.from_bytes(header[3:5], "big"))[: 5 + int.from_bytes(header[3:5], "big")]
            log_event({"event": "plaintext_request", "port": port, "peer": addr[0], "data": blob[:512].decode("latin-1", "replace")})
            return
        record_len = int.from_bytes(header[3:5], "big")
        record = peeker.ensure(5 + record_len)
        hs_len = int.from_bytes(record[6:9], "big")
        if 4 + hs_len > record_len:
            log_event({"event": "fragmented_client_hello", "port": port})
            return
        parsed = parse_client_hello(record)
        conn_info = {"event": "client_hello", "port": port, "peer": addr[0], "peer_port": addr[1], **parsed}
        log_event(conn_info)

        tls_sock = ctx.wrap_socket(conn, server_side=True)
        negotiated = {
            "event": "negotiated",
            "port": port,
            "tls_version": tls_sock.version(),
            "cipher": tls_sock.cipher()[0],
            "alpn": tls_sock.selected_alpn_protocol(),
        }
        log_event(negotiated)

        html = load_html()
        if negotiated["alpn"] == "h2":
            h2_relay(tls_sock, {**conn_info, "negotiated": negotiated}, h2c_port)
        else:
            http1_serve(tls_sock, {**conn_info, "negotiated": negotiated}, html)
    except ssl.SSLError as error:
        log_event({"event": "handshake_failed", "port": port, "error": repr(error)})
    except Exception as error:  # noqa: BLE001 - diagnostic tool, log and continue
        log_event(
            {
                "event": "handler_error",
                "port": port,
                "error": repr(error),
                "traceback": traceback.format_exc(),
                "peeked_first_bytes": peeker.all_peeked()[:96].hex(),
                "peeked_length": len(peeker.all_peeked()),
            }
        )
    finally:
        try:
            conn.close()
        except OSError:
            pass


def serve_tls(port: int, ctx: ssl.SSLContext, h2c_port: int) -> None:
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", port))
    listener.listen(16)
    log_event({"event": "listening_tls", "port": port})
    while True:
        conn, addr = listener.accept()
        threading.Thread(
            target=handle_connection, args=(conn, addr, port, ctx, h2c_port), daemon=True
        ).start()


def serve_quic_probe(port: int) -> None:
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", port))
    log_event({"event": "listening_quic_probe_udp", "port": port})
    while True:
        data, addr = sock.recvfrom(65535)
        log_event(
            {
                "event": "quic_datagram",
                "port": port,
                "peer": addr[0],
                "peer_port": addr[1],
                "length": len(data),
                "first_bytes_hex": data[:64].hex(),
                "version_field_hex": data[1:5].hex() if len(data) >= 5 else None,
            }
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tls-ports", default="8443,8444,8445,8446")
    parser.add_argument("--h2c-port", type=int, default=9443)
    parser.add_argument("--quic-port", type=int, default=8447)
    parser.add_argument("--cert", required=True)
    parser.add_argument("--key", required=True)
    args = parser.parse_args()

    RUNTIME.mkdir(exist_ok=True)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(args.cert, args.key)
    ctx.set_alpn_protocols(["h2", "http/1.1"])
    # Offer TLS 1.2 and 1.3 like a normal internet server would.
    ctx.minimum_version = ssl.TLSVersion.TLSv1_2

    threads = []
    quic_thread = threading.Thread(target=serve_quic_probe, args=(args.quic_port,), daemon=True)
    quic_thread.start()
    for port in [int(p) for p in args.tls_ports.split(",")]:
        thread = threading.Thread(target=serve_tls, args=(port, ctx, args.h2c_port), daemon=True)
        thread.start()
        threads.append(thread)
    for thread in threads:
        thread.join()
    quic_thread.join()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(0)
