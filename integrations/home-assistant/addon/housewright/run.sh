#!/bin/sh
# Starts the sync server and the bridge inside the add-on. Everything is
# kept in /data, which Home Assistant backs up with the add-on.
set -eu

OPTIONS=/data/options.json
opt() { jq -r "$1" "$OPTIONS"; }

mkdir -p /data/server /data/bridge
chmod 700 /data/server /data/bridge

pids=""

if [ "$(opt '.server')" = "true" ]; then
    set -- --listen 0.0.0.0:8787 --data-dir /data/server \
        --household-quota-mb "$(opt '.household_quota_mb')"
    if [ "$(opt '.open_registration')" = "true" ]; then
        set -- "$@" --open-registration
    fi
    if [ "$(opt '.ssl')" = "true" ]; then
        set -- "$@" --tls-cert "/ssl/$(opt '.certfile')" --tls-key "/ssl/$(opt '.keyfile')"
    else
        # Plain HTTP: phones need HTTPS, so put a proxy with a certificate
        # in front (for example the NGINX Proxy Manager add-on).
        set -- "$@" --behind-proxy
    fi
    echo "Starting the Housewright sync server on port 8787."
    echo "The registration token (typed once in the app when turning on sync) is printed below,"
    echo "and kept in the add-on's data folder as server/registration-token."
    housewright-server "$@" &
    pids="$pids $!"
fi

if [ "$(opt '.bridge')" = "true" ]; then
    # The bridge's API listens on the add-on network only (port 8788 is not
    # published to the host), where Home Assistant reaches it by the
    # add-on's host name.
    token="$(housewright-bridge --data-dir /data/bridge token)"
    echo "Starting the Housewright bridge for Home Assistant."
    echo "In Home Assistant, add the Housewright integration with:"
    echo "  address: http://$(hostname):8788"
    echo "  token:   ${token}"
    housewright-bridge --data-dir /data/bridge serve --listen 0.0.0.0:8788 &
    pids="$pids $!"
fi

if [ -z "$pids" ]; then
    echo "Both the server and the bridge are switched off in the options; nothing to do."
    exit 0
fi

trap 'kill $pids 2>/dev/null' TERM INT
# Stop when either program stops, so Supervisor can restart the add-on.
while :; do
    for p in $pids; do
        if ! kill -0 "$p" 2>/dev/null; then
            kill $pids 2>/dev/null || true
            wait || true
            exit 1
        fi
    done
    sleep 5
done
