"""Plot accepted matched stationary contact-anchor thermal controls."""

import json
from pathlib import Path
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

r = json.loads(Path("hardware/m6-hot-contact-thermal-screening.json").read_text())
rows = [r["cases"][i]["native_report"] for i in [0, 1, 4, 3]]
values = [row["temperature_cases"][0]["liquid_min_C"] for row in rows]
fig, ax = plt.subplots(figsize=(11, 6))
fig.subplots_adjust(left=0.085, right=0.98, top=0.83, bottom=0.30)
bars = ax.bar(
    [
        "No heated\nanchor",
        "30 mm² anchors\npoorer interface",
        "60 mm² anchors\npoorer interface",
        "30 mm² anchors\nnominal mica",
    ],
    values,
    color=["#ad6965", "#cfad58", "#68998b", "#68998b"],
    width=0.65,
)
ax.bar_label(bars, labels=[f"{v:.0f} °C" for v in values], padding=4)
ax.axhline(227, color="#ad5c51", ls="--", lw=1)
ax.text(
    3.49, 225, "SnCu melting point", ha="right", va="top", fontsize=9, color="#9b4c41"
)
ax.axhline(250, color="#546f82", ls=":", lw=1)
ax.text(-0.32, 254, "250 °C fixed heater", fontsize=9, color="#546f82")
ax.set(ylabel="Minimum stationary liquid temperature (°C)", ylim=(0, 275))
ax.spines[["top", "right"]].set_visible(False)
fig.suptitle(
    "Heated contact anchors raise the modeled outlet temperature\nNative 100 µm thermal experiment; outer wire ends fixed at 25 °C",
    fontsize=14,
)
fig.text(
    0.5,
    0.17,
    "Each copper extension is 50 mm long and 1 mm². Insulator thickness is 0.1 mm.",
    ha="center",
    fontsize=10,
)
fig.text(
    0.5,
    0.12,
    "Poorer interface: effective k = 0.1 W/(m K); nominal mica: k = 0.528 W/(m K). Neither is a measured clamp.",
    ha="center",
    fontsize=10,
)
fig.text(
    0.5,
    0.065,
    "Fixed properties and ideal heater reservoir; no feeding, exterior losses or Joule heat. Below-melting results flag freezing;",
    ha="center",
    fontsize=9,
)
fig.text(
    0.5,
    0.03,
    "the calculation retains liquid conductivity and does not model phase change. These are screening temperatures, not operating limits.",
    ha="center",
    fontsize=9,
)
fig.savefig(
    Path("artifacts/hardware-research/diagrams/m6-hot-contact-thermal.png"), dpi=180
)
