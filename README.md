# Port VPN

## Architecture

Not knowing if this'll scale or even worth doing but it's currently not using
threads/forks and doing as little copy and malloc as possible

### Server ( internet facing )

Listens on a publicly available port ( 8080 ) and forwards the connection
to the pvpn client

```
./pvpn server
Port VPN Server:
    Tunnel Address:  0.0.0.0:1414
    Server Address:  0.0.0.0:8080
```

### Client ( NAT'ed or Firewalled )

Establish a connection with the pvpn server and creates a tunnel to expose
a local service

```
./pvpn client  --tunnel-address 1.2.3.4 --server-address 127.0.0.1 --server-port 1234
Port VPN Client:
    Tunnel Server:   1.2.3.4:1414
    Server:          127.0.0.1:1234
    Reconnect:       500 ms
```

## N.B

- Tunnel doesn't offer compression or crypto (yet?) This is currently just
  a personal project and the internal server is already offering TLS
