# Raspberry Pi testbed and provisional shopping list

The Raspberry Pi is the planned deployment and measurement platform for the
assurance runtime. The building simulation remains the first demonstration of a
general assurance system. Running the simulator on a Pi measures its behaviour
and processing costs on that device; adding physical inputs and outputs creates
a hardware-in-the-loop demonstration. Neither step alone establishes the 80%
decision-correctness target.

We already own a Pi 5 and a Pi 4 and will use both. The Pi 5 will host the
assurance runtime, experiment orchestration and dashboard. The Pi 4 will host
sensor acquisition and the local actuator/gateway adapter. This is the planned
architecture; live GPIO and distributed adapters still need implementation.
Purchase quantities below are total requirements: subtract equipment already owned.

## Two-board purchase specification

### Power, cooling, storage and networking

| Item | Quantity | Purchase specification and purpose |
| --- | --- | --- |
| Pi 5 supply | 1 | Official 27W USB-C supply, 5.1 V / 5 A profile; appropriate mains plug |
| Pi 4 supply | 1 | Official 15W USB-C supply, 5.1 V / 3 A; appropriate mains plug |
| Pi 5 cooling | 1 | Official Active Cooler plus a case explicitly compatible with it, or a suitable fan-equipped case instead |
| Pi 4 cooling/case | 1 set | Pi 4-specific ventilated case with heatsinks and fan; GPIO must remain accessible |
| microSD cards | 2 | 64 GB each, reputable brand, A1/A2 and Class 10/U1 or better; one per board |
| USB microSD reader | 1 | Compatible with the development laptop's USB port |
| Ethernet cables | 3 | Cat5e or Cat6: two Pis and a laptop/uplink connection |
| Gigabit Ethernet switch | 1 if needed | Five ports, unmanaged, supplied with its power adapter; reuse existing router LAN ports instead if available |
| Power strip | 1 | At least six sockets for both Pis, actuator supply, network switch and development equipment |

Verified supply references: [Pi 5 supply](https://www.raspberrypi.com/products/27w-power-supply/)
and [Pi 4 supply](https://www.raspberrypi.com/products/type-c-power-supply/).
Card capacity and networking choices are project recommendations, not minimum
manufacturer requirements. A separate router is unnecessary if an existing router
provides DHCP; an isolated switch network can also use configured static addresses.

### Wiring, controlled inputs and measurement

| Item | Quantity | Purchase specification and purpose |
| --- | --- | --- |
| Full-size solderless breadboards | 2 | Approximately 830 tie points each; low-current signal circuits |
| Jumper wires | 3 packs | One pack each male-male, male-female and female-female; approximately 40 wires per pack |
| Pi GPIO breakout | 1 set | Labelled 40-pin breakout and matching ribbon cable for Pi 4; keyed connector preferred |
| Toggle switches | 6 | Simple maintained SPST/SPDT contacts: occupancy, fault states and manual test conditions |
| Pushbuttons | 6 | Momentary buttons for scenario controls and acknowledgement inputs |
| LEDs | 15 | Five each red, green and yellow; decision, connection and action indicators |
| Resistors | 1 assortment | Include 1 kΩ, 4.7 kΩ and 10 kΩ, at least 20 each; 1/4 W is sufficient for these signal circuits |
| Screw-terminal connectors | 1 kit | Power distribution and secure low-voltage connections |
| Hookup wire | 1 kit | 22–24 AWG signal wire; separate 18–20 AWG short actuator supply wiring, subject to actual current |
| Digital multimeter | 1 | DC voltage, resistance and continuity; fused current input if measuring current |
| Hand tools | 1 set | Wire stripper, small cutters and small screwdriver set |
| Mounting base and hardware | 1 set | Nonconductive base, standoffs, screws, cable ties and labels |

Keep motor and servo supply currents off breadboard power rails. Prefer modules
with pre-soldered headers. If buying bare-header boards, add a temperature-controlled
soldering iron, stand, solder and heat-shrink tubing.

### Real evidence sensors

| Item | Quantity | Purchase specification and purpose |
| --- | --- | --- |
| MCP9808 temperature breakout | 3 | 3.3 V-compatible I2C boards with configurable addresses and pre-soldered headers; three evidence channels for quorum experiments |
| Lever microswitches | 4 | Two end positions each for the model door and damper; buy switches with COM/NO/NC contacts |
| Independent reference thermometer | 1 | Prefer a documented accuracy at least as good as the scoring tolerance; must not feed the admission decision |

Use separate addresses such as 0x18, 0x19 and 0x1A for the temperature sensors.
The [MCP9808 guide](https://learn.adafruit.com/adafruit-mcp9808-precision-i2c-temperature-sensor-guide?view=all)
documents its eight selectable addresses. Power the selected breakouts at 3.3 V
and check that I2C pull-ups go to 3.3 V. Keep the I2C wiring short.

These sensors support a temperature-threshold demonstration, not fire detection.
Use controlled modest temperature changes and software-injected disagreements.
Three identical sensors on one bus demonstrate quorum logic but share failure
modes. Use switches for deterministic occupancy scenarios; a PIR sensor is optional
for motion experiments and must not be treated as proof of absence.

### Physical actions and feedback

| Item | Quantity | Purchase specification and purpose |
| --- | --- | --- |
| Positional micro servos | 2 | Documented 5 V operation and usable 3.3 V control input; SG90-class devices from a supplier with specifications; include horns and screws; no continuous-rotation versions |
| PCA9685 servo driver | 1 | I2C breakout with separate VCC and servo V+ terminals, pre-soldered servo headers; 3.3 V logic supply |
| Small 5 V fan | 1 | Guarded low-current fan with documented start current; choose a model with documented tachometer output if rotation is to be measured |
| Fan switching driver | 1 | Assembled MOSFET load-switch module rated for the fan's startup current, explicitly specified to switch fully from 3.3 V GPIO; compatible with the fan type |
| Regulated actuator supply | 1 | Enclosed 5 V / 5 A DC supply as a proposed starting choice; verify combined servo stall and fan startup demand before purchase |
| Supply connector adapter | 1 | Match actual barrel diameter and polarity to screw terminals, or use the supply's provided terminal connection |
| Actuator power cutoff switch | 1 | DC-rated for the supply/load current; disconnect actuator positive power without powering down the Pis |
| Inline fuse holder and fuses | 1 set | Size fuse to the selected load and wiring; place near actuator supply output |
| Small model mechanism | 1 set | Lightweight hinged door and damper, servo brackets/linkages and end-switch mounts |
| Fan feedback interface | 1 if needed | For an open-collector tachometer, 3.3 V pull-up; for other outputs, suitable level conversion according to the fan datasheet |

The [PCA9685 pinout guide](https://learn.adafruit.com/16-channel-pwm-servo-driver/pinouts)
distinguishes logic VCC from servo V+. Use Pi 3.3 V for logic and the separate
actuator supply for servo power. Connect signal grounds appropriately on the Pi 4
side; keep the two Pis connected through Ethernet rather than GPIO wiring.

A motor driver for a brushed DC motor is an alternative if using a small motor
and guarded propeller instead of an electronic fan. For example, the
[Pololu DRV8833 carrier](https://www.pololu.com/product/2130)
is specified for low-voltage brushed motors. Do not assume it is suitable for
every brushless fan. Motor/fan switching and feedback parts must be matched to
the exact selected actuator.

Position switches and fan tachometer feedback report effects independently of
the command. A servo command does not prove the door moved; a powered fan does
not establish airflow or pressure. Leave pressure simulated until a measured
pressure experiment, differential sensor, tubing and reference procedure are designed.
Keep suppression/discharge as an LED or simulated effect.

### Useful optional equipment

- One UPS sized for both Pi supplies, network equipment and actuator supply if
  unreliable mains would interrupt experiments. Log backup operation and deliberate
  power-loss tests separately.
- One micro-HDMI-to-HDMI cable and shared display/USB keyboard/mouse for local
  troubleshooting; reuse existing equipment or use headless access.
- One 3.3 V-compatible USB logic analyser for I2C/GPIO troubleshooting; check
  input ratings and software support for the development laptop.
- One PIR module with documented Pi-compatible output for motion experiments.
- Spare microSD card, servo and sensor for replacement rather than extra nodes.

No GPU, AI accelerator, NVMe drive, camera, Arduino or ESP32 is required for this
initial two-Pi testbed. Software can inject message delay, loss, duplication and
reordering on the live link; no specialist network fault appliance is required.

## What this kit lets us benchmark

| Test | Equipment and scoring approach |
| --- | --- |
| Correct release / withholding | Labelled switches and controlled temperature readings against predefined expected decisions |
| Alternative evidence / quorum | Three sensor channels, with declared individual and shared-bus faults |
| Stale or lost observations | Pi 4 publisher and controlled transport/publisher faults; log versions and expiry |
| Network partition and recovery | Two Pi Ethernet link with software-controlled disconnection/faults |
| Actual command execution | Servo position switches and fan rotation feedback |
| Missing acknowledgement | Withhold result messages while independently recording the physical effect |
| Processing cost | Pi 5 host timings, separate from network transit and virtual simulation time |

For cross-device latency measurements, record clock synchronisation uncertainty
or use round-trip measurements. Preserve a scoring trace separately from runtime
evidence, including independent reference readings and observed physical effects.
Hardware purchase does not by itself establish the 80% target.

## Implementation and benchmark work

1. Establish a reproducible Pi setup and run the existing tests and matched
   baseline experiments. Record Pi model, OS, build, cooling and power setup.
2. Measure host processing on the Pi separately from virtual simulation time.
   Record thermal conditions and repeated-run variability.
3. Add observation and actuator adapters while preserving versioning, expiry,
   contracts and correlated outcome handling. Live operation needs monotonic
   elapsed time and physical I/O scheduling beyond the existing virtual clock.
4. Keep experiment scoring independent of the runtime. Record expected decisions,
   actual decisions, effects, missed useful actions and unconfirmed outcomes.
5. Re-run matched comparisons and evaluate decision correctness with uncertainty
   estimates. A successful hardware demonstration does not replace that evaluation.

Purchase the two-board essentials, evidence inputs and selected physical-action kit above.
Final fan, servo, driver and power specifications must be checked together before ordering.
