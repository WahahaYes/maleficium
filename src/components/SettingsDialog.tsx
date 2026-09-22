// SettingsDialog.tsx — appearance controls over the prefs object.
//
// Every control writes prefs; persistence lives with the caller. Themes
// with both variants pair into one row; sizes offer named presets plus
// a slider override.

import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import Chip from '@mui/material/Chip';
import List from '@mui/material/List';
import ListItemButton from '@mui/material/ListItemButton';
import ListItemText from '@mui/material/ListItemText';
import Slider from '@mui/material/Slider';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';
import Typography from '@mui/material/Typography';
import Box from '@mui/material/Box';
import type { AppearancePrefs } from '../lib/appearance';
import { THEME_CATALOG } from '../lib/themeCatalog';

/** Theme families shipping both variants. */
const PAIRS: { family: string; dark: string; light: string }[] = [
  { family: 'Solarized', dark: 'solarized-dark', light: 'solarized-light' },
  { family: 'Night Owl', dark: 'night-owl', light: 'owl-light' },
];

const PAIRED_IDS = new Set(PAIRS.flatMap((p) => [p.dark, p.light]));

function SizeRow({
  label,
  value,
  presets,
  min,
  max,
  step,
  format,
  onPick,
}: {
  label: string;
  value: number;
  presets: [string, number][];
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onPick: (v: number) => void;
}) {
  const matched = presets.find(([, v]) => v === value)?.[0] ?? null;
  return (
    <Box sx={{ mt: 2 }}>
      <Typography variant="caption">
        {label} · {format(value)}
        {matched ? '' : ' (custom)'}
      </Typography>
      <ToggleButtonGroup
        exclusive
        fullWidth
        size="small"
        sx={{ '& .MuiToggleButton-root': { flex: 1 } }}
        value={matched}
        onChange={(_, v: string | null) => {
          const hit = presets.find(([name]) => name === v);
          if (hit) onPick(hit[1]);
        }}
        aria-label={label}
      >
        {presets.map(([name]) => (
          <ToggleButton key={name} value={name}>
            {name}
          </ToggleButton>
        ))}
      </ToggleButtonGroup>
      <Slider
        size="small"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(_, v) => onPick(v as number)}
        aria-label={`${label} fine tune`}
        sx={{ mt: 1 }}
      />
    </Box>
  );
}

export default function SettingsDialog({
  open,
  onClose,
  prefs,
  onChange,
}: {
  open: boolean;
  onClose: () => void;
  prefs: AppearancePrefs;
  onChange: (p: AppearancePrefs) => void;
}) {
  const set = (patch: Partial<AppearancePrefs>) => onChange({ ...prefs, ...patch });
  return (
    <Dialog open={open} onClose={onClose} maxWidth="xs" fullWidth>
      <DialogTitle>Appearance</DialogTitle>
      <DialogContent>
        <Box>
          <Typography variant="caption">Theme</Typography>
          <List dense disablePadding>
            <ListItemButton
              selected={prefs.accent === 'default'}
              onClick={() => set({ accent: 'default' })}
              sx={{ gap: 1.5, py: 1 }}
            >
              <ListItemText
                primary="Default"
                secondary="Dark + Light"
                sx={{ minWidth: 0 }}
                slotProps={{ primary: { noWrap: true } }}
              />
              <ToggleButtonGroup
                exclusive
                size="small"
                value={prefs.accent === 'default' ? prefs.mode : null}
                onClick={(e) => e.stopPropagation()}
                onChange={(_, v) => v && set({ accent: 'default', mode: v })}
                aria-label="Default variant"
                sx={{ flexShrink: 0 }}
              >
                <ToggleButton value="dark">Dark</ToggleButton>
                <ToggleButton value="light">Light</ToggleButton>
              </ToggleButtonGroup>
            </ListItemButton>
            {PAIRS.map((p) => {
              const active =
                prefs.accent === p.dark ? 'dark' : prefs.accent === p.light ? 'light' : null;
              return (
                <ListItemButton
                  key={p.family}
                  selected={active !== null}
                  onClick={() => active === null && set({ accent: p.dark })}
                  sx={{ gap: 1.5, py: 1 }}
                >
                  <ListItemText
                    primary={p.family}
                    secondary="Dark + Light"
                    sx={{ minWidth: 0 }}
                    slotProps={{ primary: { noWrap: true } }}
                  />
                  <ToggleButtonGroup
                    exclusive
                    size="small"
                    value={active}
                    onClick={(e) => e.stopPropagation()}
                    onChange={(_, v) => v && set({ accent: v === 'dark' ? p.dark : p.light })}
                    aria-label={`${p.family} variant`}
                    sx={{ flexShrink: 0 }}
                  >
                    <ToggleButton value="dark">Dark</ToggleButton>
                    <ToggleButton value="light">Light</ToggleButton>
                  </ToggleButtonGroup>
                </ListItemButton>
              );
            })}
            {THEME_CATALOG.filter((t) => !PAIRED_IDS.has(t.id)).map((t) => (
              <ListItemButton
                key={t.id}
                selected={prefs.accent === t.id}
                onClick={() => set({ accent: t.id })}
                sx={{ gap: 1.5, py: 1 }}
              >
                <ListItemText
                  primary={t.name}
                  sx={{ minWidth: 0 }}
                  slotProps={{ primary: { noWrap: true } }}
                />
                <Chip size="small" label={t.type === 'dark' ? 'Dark' : 'Light'} />
              </ListItemButton>
            ))}
          </List>
        </Box>
        <Box sx={{ mt: 2 }}>
          <Typography variant="caption">Density</Typography>
          <ToggleButtonGroup
            exclusive
            fullWidth
            size="small"
            sx={{ '& .MuiToggleButton-root': { flex: 1 } }}
            value={prefs.density}
            onChange={(_, v) => v && set({ density: v })}
            aria-label="Density"
          >
            <ToggleButton value="comfortable">Comfortable</ToggleButton>
            <ToggleButton value="compact">Compact</ToggleButton>
          </ToggleButtonGroup>
        </Box>
        <Box sx={{ mt: 2 }}>
          <Typography variant="caption">Corner radius</Typography>
          <ToggleButtonGroup
            exclusive
            fullWidth
            size="small"
            sx={{ '& .MuiToggleButton-root': { flex: 1 } }}
            value={prefs.radius}
            onChange={(_, v) => v && set({ radius: v })}
            aria-label="Corner radius"
          >
            {[6, 8, 12].map((r) => (
              <ToggleButton key={r} value={r}>
                {r}
              </ToggleButton>
            ))}
          </ToggleButtonGroup>
        </Box>
        <SizeRow
          label="Interface scale"
          value={prefs.uiScale}
          presets={[
            ['S', 0.875],
            ['M', 1],
            ['L', 1.125],
          ]}
          min={0.75}
          max={1.5}
          step={0.125}
          format={(v) => `${Math.round(v * 100)}%`}
          onPick={(v) => set({ uiScale: v })}
        />
        <SizeRow
          label="Editor size"
          value={prefs.editorSize}
          presets={[
            ['S', 12],
            ['M', 14],
            ['L', 16],
          ]}
          min={11}
          max={18}
          step={1}
          format={(v) => `${v}px`}
          onPick={(v) => set({ editorSize: v })}
        />
      </DialogContent>
    </Dialog>
  );
}
