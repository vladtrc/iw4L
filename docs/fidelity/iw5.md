# Modern Warfare 3 — fidelity ledger

Rule: Modern Warfare 3 matches run on Modern Warfare 3's own rules and data;
no other game's behaviour stands in. A rule that is not recovered is an
`unknown!` in `game_iw5`, listed here with its id; `cargo xtask boundary`
refuses an id that is missing here.

Severity: **V** visible, **A** audible, **G** gameplay, **I** invisible.

## Open

- [ ] **G** `iw5.scripts.gametypes` (`GameScripts::program`): no Modern
      Warfare 3 map has a script program, so its matches are refused at load.
      Until then they ran Modern Warfare 2's gametype scripts and natives.
      *Needs:* Modern Warfare 3's builtin list and the natives behind it, and
      its match flow.

- [ ] **V** `iw5.hud.menu_font` (`GameMenus::font`): which font a Modern Warfare 3 menu
      item's `textfont` names is not known; such text is not drawn. (Until
      now Modern Warfare 2's font table answered for any non-Black Ops
      catalog.) *Needs:* Modern Warfare 3's menu font table.

- [ ] **V** `iw5.hud.menu_layout` (`GameMenus::layout`): how Modern Warfare 3's menu
      engine places items is not known; its menus are not drawn (no such
      catalog is painted today). *Needs:* Modern Warfare 3's menu engine rules.

- [ ] **V** `iw5.vision.shellshock` (`GameVision::shellshock`): Modern Warfare 3's
      shellshock files and screen rules are not known, so its matches install
      no shellshock (a script `shellshock` call fails as unknown). Until now
      they got Modern Warfare 2's `.shock` files, read with Modern Warfare 2's
      format. *Needs:* Modern Warfare 3's shellshock format and screen rules.

## Closed
