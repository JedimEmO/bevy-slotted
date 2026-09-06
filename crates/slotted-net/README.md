# slotted-net

A networked [`Authority`](https://docs.rs/slotted-model) for
[slotted](https://github.com/mathiasmyrland/bevy-slotted): the client predicts,
the server decides, and neither one knows what carries the bytes between them.

* `RemoteAuthority` is the client half. It implements `slotted_model::Authority`,
  so dropping it in place of `LocalAuthority` is the whole change a game makes.
* `MenuServer` is the server half. It validates every click with the same
  `apply_click` the client ran, at `ValidationLevel::Always`, against its own
  copy of the menu.
* `Transport` is the port between them. `Loopback` is the in-process adapter,
  and it can add latency, reorder and drop messages so a test can see what a
  bad connection does.

See `docs/PLAN.md` section 4.1 and `docs/design/gaps-notes-A.md`.
