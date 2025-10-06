# Port VPN

## Architecture

Not knowing if this'll scale or even worth doing but it's currently not using
threads/forks and doing a litle copy and malloc as possible

### Server ( internet facing )

Listens on a publicly available port ( 8080 ) and forwards the connection
to the pvpn client

### Client ( NAT'ed or Firewalled )

Establish a connection with the pvpn server and creates a tunnel to expose
a local service

## N.B

- Tunnel doesn't offer compression or crypto (yet?) This is currently just
  a personal project and the internal server is already offering TLS
