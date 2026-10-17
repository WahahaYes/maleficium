"""Write the paper's Plotly figures (charts/*.plotly.json) from data/.

Each is what fig.write_json() saves. One keeps Plotly Express defaults and
takes the paper's style; the other brings its own template, which the widget
keeps with template=figure. Needs Python 3 with plotly and pandas, e.g.
uv run --no-project --with plotly --with pandas tools/plotly_figures.py
"""
import os

import pandas as pd
import plotly.express as px
import plotly.graph_objects as go

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = os.path.join(ROOT, "charts")
results = pd.read_csv(os.path.join(ROOT, "data/results.csv"))


def save(fig, name):
    fig.write_json(os.path.join(OUT, f"{name}.plotly.json"))


# Plotly Express defaults throughout: the paper's palette, backdrop and
# fonts replace them, in both colour modes.
fig = px.scatter(
    results,
    x="latency",
    y="accuracy",
    text="method",
    labels={"latency": "latency (ms)", "accuracy": "accuracy"},
)
fig.update_traces(mode="lines+markers+text", textposition="bottom right")
save(fig, "tradeoff")

# Its own template: palette, font, backdrop and grid. template=figure in the
# widget keeps all of it, in both modes.
house = go.layout.Template(
    layout=dict(
        colorway=["#2A9D8F", "#E9C46A", "#F4A261", "#E76F51", "#264653"],
        font=dict(family="Georgia, serif", color="#264653"),
        paper_bgcolor="#FAF6EE",
        plot_bgcolor="#FAF6EE",
        xaxis=dict(gridcolor="#E6DCC8", zeroline=False),
        yaxis=dict(showgrid=False),
        hoverlabel=dict(font_family="Georgia, serif"),
    )
)
fig = px.bar(
    results,
    x="latency",
    y="method",
    color="method",
    orientation="h",
    template=house,
    labels={"latency": "latency (ms)"},
)
fig.update_layout(showlegend=False, bargap=0.3, yaxis_title_text="")
save(fig, "latency")
