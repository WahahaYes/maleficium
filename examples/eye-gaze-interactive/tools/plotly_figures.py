"""Write the appendix's Plotly figures (charts/*.plotly.json) from data/.

Each is what fig.write_json() saves, so it shows how a Python figure looks
in the paper: two keep Plotly's defaults and take the paper's style, one
sets its own palette and font, one keeps its own template. Needs Python 3
with plotly and pandas. Run: python3 -I tools/plotly_figures.py
"""
import os

import pandas as pd
import plotly.express as px

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = os.path.join(ROOT, "charts")
e1e2 = pd.read_csv(os.path.join(ROOT, "data/e1e2.csv"))
vis = pd.read_csv(os.path.join(ROOT, "data/vis_size.csv"))


def save(fig, name):
    fig.write_json(os.path.join(OUT, f"{name}.plotly.json"))


# Plotly Express defaults throughout: the paper's palette, plate and fonts
# replace them, in both colour modes.
e1e2["plus"] = e1e2.ci_high - e1e2.vlm
e1e2["minus"] = e1e2.vlm - e1e2.ci_low
fig = px.line(
    e1e2,
    x="fixations",
    y="vlm",
    color="experiment",
    markers=True,
    error_y="plus",
    error_y_minus="minus",
    labels={"fixations": "prior fixations as context", "vlm": "VLM accuracy (%)"},
)
save(fig, "accuracy")

# A heatmap with no colourscale of its own takes the paper's sequential one.
vis["curve"] = vis.measure + " · " + vis.space
grid = vis.pivot(index="curve", columns="pct", values="deg")
fig = px.imshow(
    grid,
    aspect="auto",
    labels={"x": "objects covered (percentile)", "y": "", "color": "degrees"},
)
save(fig, "coverage")

# The figure's own palette and font win; the plate, ink and grid stay the
# paper's.
at6 = e1e2[e1e2.fixations == 6].melt(
    id_vars="experiment",
    value_vars=["vlm", "greedy", "random_prior", "random_visible"],
    var_name="method",
    value_name="accuracy",
)
fig = px.bar(
    at6,
    x="experiment",
    y="accuracy",
    color="method",
    barmode="group",
    color_discrete_sequence=["#2A9D8F", "#E9C46A", "#F4A261", "#E76F51"],
    labels={"accuracy": "accuracy at 6 fixations (%)", "experiment": ""},
)
fig.update_layout(
    font_family="Helvetica, Arial, sans-serif",
    bargap=0.25,
    legend=dict(orientation="h", yanchor="bottom", y=1.02, x=0, title_text=""),
)
save(fig, "baselines")

# template=figure in the widget keeps this ggplot2 look in both modes.
fig = px.area(
    e1e2,
    x="fixations",
    y="random_prior",
    color="experiment",
    template="ggplot2",
    labels={"fixations": "prior fixations as context", "random_prior": "random prior (%)"},
)
save(fig, "random-prior")
