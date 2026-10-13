# dormer

A dormer is the window that sits in a roof. This one is your wallpaper: the
same view every day, under the sky that is actually over you right now.

dormer is one small native program. It paints a place from things that are true
at this moment, and keeps it alive on your desktop: the sun and moon where they
really are, tonight's real stars and planets, the weather outside, the rhythm
of the week, and a few signals from your own machine.

![The three views by day, at sunset and at night](docs/views.jpg)

## Three views

The sky is the same in all of them. What stands under it is yours to choose:

| | |
| --- | --- |
| `rooftops` | The old town from a window in the roof: painted walls, tiled roofs, a clock tower, a domed hall. |
| `city` | The middle of town across a railway viaduct: towers placed by hand, offices that keep office hours, trains to a timetable. |
| `coast` | A headland and its lighthouse over the strait: the sea mirrors the sky, and the tide follows the moon. |

```sh
dormer view            # list them
dormer view coast      # look out there from now on; the wallpaper fades over
dormer view default    # back to what the config says
```

Your machine shows in the same four ways everywhere, staged for the place:

| | rooftops | city | coast |
| --- | --- | --- | --- |
| An AI coding agent is running | the lamp in the roof room | the lantern on the tallest tower | the lamp in the keeper's window |
| Each running Docker container | a blue window under it | a blue floor under it | a blue lantern on a moored boat |
| The processor is working hard | smoke from its stovepipe | steam off the tower's roof | smoke from the lean-to's stovepipe |
| Sound is playing | notes drift from it | notes drift from it | notes drift from it |

The lamp burns low and red when the battery is nearly flat.

## What the sky is telling you

| You see | It means |
| --- | --- |
| The sun, the colour of the sky | The real sun over your latitude. It rises on the left and sets on the right. |
| The moon | Its real position and phase, with the bright limb turned towards the sun. |
| Stars, a few with a sparkle | The real brightest stars (Orion in winter, the Summer Triangle in summer), turning through the night. Steady bright points are Venus, Mars, Jupiter and Saturn. On dark moonless nights the Milky Way shows where it really lies. |
| More shooting stars than usual | A real meteor shower is at its peak. |
| Cloud, rain, snow, fog, lightning | The current weather. Clouds drift with the wind that is blowing. |
| A dark bank of cloud on one side | Rain is forecast within the next few hours; it builds on the side the wind comes from. |
| Mist lying low, frost on the roofs | Still, damp air near sunrise; a freezing night. |
| Windows lit or dark | Every window keeps its own hours. Homes wake, empty out and go to bed; offices light up on working days only. |
| How many planes cross, the crane turning | A working day. Monday starts earliest and is busiest. |
| `z z z`, washing on a line, a balloon, a kite | The weekend. |
| The cat | Somewhere different every hour and a half, asleep at night, indoors when it rains. |

And in each view, a few things of its own:

| | You see | It means |
| --- | --- | --- |
| rooftops | The clock tower | The real time. On the hour the birds on it take off. |
| | String lights on a roof | Friday and Saturday night, and every night in late December. Fireworks at New Year. |
| city | The station clock | The real time. |
| | A train on the viaduct | The timetable: every three minutes at the rush on a working day, every nine at the weekend, none between half past midnight and five. |
| | The light on the old exchange's mast | A weather beacon. Steady blue: fair ahead. Flashing blue: cloud about. Red: rain now or on the way. |
| | Lines of light along the rooflines | After dark, until one in the morning. |
| coast | Rocks and a sandbar showing | Low water. The tide is worked out from where the moon is: about right for an open coast, not a tide table. |
| | A path of light on the water | The low sun, or a bright moon. |
| | A ferry on the horizon | It left on the hour, and takes twelve minutes to cross. |
| | Small boats with lamps; sails | Fishing before sunrise on a working day; sails out at the weekend when there is wind. |
| | A fire on the dune | Friday and Saturday night. |

`dormer explain` says the same in words.

![Snow, rain, a storm, fog, mist and an overcast day](docs/weather.jpg)

## Install

Rust, and a Wayland compositor with layer-shell (developed on Hyprland).

```sh
git clone <this repository> && cd dormer
cargo build --release
install -Dm755 target/release/dormer ~/.local/bin/dormer

dormer doctor      # what dormer found: place, weather source, wallpaper daemon
dormer install     # the living wallpaper on login, and a still copy every five minutes
```

`dormer install` writes three systemd user units. `dormer live` is the living
wallpaper; `dormer draw` (on a timer) hands a still frame to your wallpaper
daemon, which is what a lock screen shows and what remains if `dormer live` is
not running. `dormer install --no-live` keeps only the still frames.

## Commands

`dormer --help` lists everything. The ones you will use:

| | |
| --- | --- |
| `dormer view city` | Look out somewhere else: `rooftops`, `city` or `coast`. |
| `dormer explain` | Describe the scene in words. Takes `--at`, `--weather` and `--view`. |
| `dormer tour` | Play today from midnight to midnight, smoothly, in about a minute. |
| `dormer pretend snow` | Borrow some weather for ten minutes. |
| `dormer draw --out a.png --at "2026-12-31 23:50"` | Draw another moment to a file. |
| `dormer sheet out.png "2026-10-13 07:10,city" "2026-10-13 21:00,storm,coast"` | Several moments on one sheet. |
| `dormer hold on` / `off` | Freeze or release the living wallpaper (for a lock screen's hooks). |
| `dormer dev` | A floating panel: choose the view, then sliders for every input, and the view's own knobs. The wallpaper follows as you drag. |
| `dormer doctor` | Show what is in effect on this machine. |

## What it costs

Measured on a 1920x1200 laptop screen (Core Ultra 5):

| | |
| --- | --- |
| Painting one still frame, any view | 15 to 17 ms (about 21 ms with the final pass), once every twenty seconds |
| Living wallpaper, a calm scene in view | 0.6 to 1.5% of one core |
| Rain or snow in view | about 7% of one core: the whole screen changes every frame |
| Wallpaper covered by a tiled or fullscreen window | nothing |
| Memory | about 35 MB |

Only the rectangles the moving things touch are repainted and reported to the
compositor. Under Hyprland it stops the moment a window covers the workspace.

## Configuration

Everything is optional; see [`config.example.toml`](config.example.toml).
With the Noctalia shell running, dormer reads its location and cached weather
and makes no network requests. Without it, set `[place]` and dormer asks
[Open-Meteo](https://open-meteo.com) at most every twenty minutes (through
`curl`; your coordinates are sent to that service).

The `swww`, `hyprpaper` and `command` backends for the still frame are
implemented from their documentation and have not been tested.

## How it works

- `sky.rs` computes the sun, moon and planets with low-precision formulas and
  holds a small catalogue of the brightest stars.
- `scene.rs` turns a time, the weather and the machine into colours and facts.
- `gfx.rs` is dormer's own renderer: exact-area antialiasing, floating-point
  compositing, bloom, and a fixed grain that hides banding.
- `paint.rs` paints the sky, then hands over to the view for the ground.
- `views/` holds the views. Each says where its horizon is, lays its ground
  out once for a screen size, paints it, and draws the few things of its own
  that move. `views/kit.rs` turns a hand-written table of buildings into a
  skyline with windows that keep hours.
- `live.rs` simulates what is in the air, and animates whatever a view points
  at while painting: a chimney here, a television behind that window, a
  beacon there. `wl.rs` puts it on a layer surface.
- `dev.rs` is the panel. `dormer soak` draws thousands of random moments in
  every view to shake out faults; `cargo test` checks the sky and the hours
  people keep.

### Adding a view

Write a module in `src/views/` with a type that implements `View` (its name,
its horizon, its knobs for the dev panel, its words for `dormer explain`) and
a `Stage` (lay out, paint the ground, draw what moves), and add it to `ALL` in
`views/mod.rs`. The command line, the config, the dev panel and the soak test
pick it up from there. `coast.rs` is the one to read for land drawn by hand;
`downtown.rs` for buildings.

Setting `DORMER_TIME` prints how long each stage of a still frame took;
`DORMER_PROBE` makes `dormer live` report where its time goes every five
seconds.

## Ideas not built yet

Rain on its own half-resolution surface (to cut its cost), car lights in the
streets, trees swaying, an owl and bats at dusk, the ISS on its real passes,
puddle sheen on wet roofs, crepuscular rays, configurable holidays, a view
that changes with the day of the week, waves breaking on the rocks in a gale.

`prototype/` holds the original Python version this was rewritten from.
