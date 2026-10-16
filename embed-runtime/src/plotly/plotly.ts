// plotly.js with the traces Plotly draws as SVG and that need no network:
// no WebGL (it compiles shaders through new Function, which the widget
// policy forbids) and no maps or globes (they fetch tiles). Keep in step
// with TRACES in ./core.ts (core.test.ts checks).
import Plotly from 'plotly.js/lib/core';
import bar from 'plotly.js/lib/bar';
import barpolar from 'plotly.js/lib/barpolar';
import box from 'plotly.js/lib/box';
import candlestick from 'plotly.js/lib/candlestick';
import carpet from 'plotly.js/lib/carpet';
import contour from 'plotly.js/lib/contour';
import contourcarpet from 'plotly.js/lib/contourcarpet';
import funnel from 'plotly.js/lib/funnel';
import funnelarea from 'plotly.js/lib/funnelarea';
import heatmap from 'plotly.js/lib/heatmap';
import histogram from 'plotly.js/lib/histogram';
import histogram2d from 'plotly.js/lib/histogram2d';
import histogram2dcontour from 'plotly.js/lib/histogram2dcontour';
import icicle from 'plotly.js/lib/icicle';
import indicator from 'plotly.js/lib/indicator';
import ohlc from 'plotly.js/lib/ohlc';
import parcats from 'plotly.js/lib/parcats';
import pie from 'plotly.js/lib/pie';
import sankey from 'plotly.js/lib/sankey';
import scattercarpet from 'plotly.js/lib/scattercarpet';
import scatterpolar from 'plotly.js/lib/scatterpolar';
import scattersmith from 'plotly.js/lib/scattersmith';
import scatterternary from 'plotly.js/lib/scatterternary';
import sunburst from 'plotly.js/lib/sunburst';
import table from 'plotly.js/lib/table';
import treemap from 'plotly.js/lib/treemap';
import violin from 'plotly.js/lib/violin';
import waterfall from 'plotly.js/lib/waterfall';

Plotly.register([
  bar,
  barpolar,
  box,
  candlestick,
  carpet,
  contour,
  contourcarpet,
  funnel,
  funnelarea,
  heatmap,
  histogram,
  histogram2d,
  histogram2dcontour,
  icicle,
  indicator,
  ohlc,
  parcats,
  pie,
  sankey,
  scattercarpet,
  scatterpolar,
  scattersmith,
  scatterternary,
  sunburst,
  table,
  treemap,
  violin,
  waterfall,
]);

export default Plotly;
