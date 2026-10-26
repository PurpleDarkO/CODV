# CODV patches to iw4L

`tools/prepare_overlay.py` copies the iw4L crates CODV compiles (from `upstream/iw4L`, pinned in
`upstreams.lock.json`) into `build/overlay/iw4l` and applies every `*.patch` here in name order with
`git apply -p1`, paths relative to the iw4L repository root (`crates/movement_iw4/src/...`).

Make a patch from a change in the overlay:

    git -C build/overlay/iw4l init -q && git -C build/overlay/iw4l add -A && git -C build/overlay/iw4l commit -qm base
    (edit files under build/overlay/iw4l)
    git -C build/overlay/iw4l diff > patches/iw4l/0001-short-name.patch

Number them in order (`0001-`, `0002-`, ...), one concern each. The series is empty today: CODV uses the
pinned crates unchanged.
