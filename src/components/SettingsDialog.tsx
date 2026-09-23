// SettingsDialog.tsx — appearance controls over the prefs object.
//
// Every control writes prefs; persistence lives with the caller. The
// theme picker is a searchable dropdown grouped by family; sizes offer
// named presets plus a slider override.

import Autocomplete from '@mui/material/Autocomplete';
import DarkModeIcon from '@mui/icons-material/DarkMode';
import LightModeIcon from '@mui/icons-material/LightMode';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import Slider from '@mui/material/Slider';
import TextField from '@mui/material/TextField';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';
import Typography from '@mui/material/Typography';
import Box from '@mui/material/Box';
import type { AppearancePrefs } from '../lib/appearance';
import { THEME_CATALOG, type ThemeEntry } from '../lib/themeCatalog';

type ThemeOption = Pick<ThemeEntry, 'id' | 'name' | 'type' | 'family'>;

const DEFAULT_OPTION: ThemeOption = {
  id: 'default',
  name: 'Default',
  type: 'dark',
  family: 'Built-in',
};

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
        <Box sx={{ mt: 2 }}>
          <Autocomplete
            size="small"
            disableClearable
            disableCloseOnSelect
            options={[
              { ...DEFAULT_OPTION, type: prefs.mode },
              ...[...THEME_CATALOG].sort((a, b) => a.name.localeCompare(b.name)),
            ]}
            getOptionLabel={(o) => o.name}
            isOptionEqualToValue={(o, v) => o.id === v.id}
            value={
              prefs.accent === 'default'
                ? { ...DEFAULT_OPTION, type: prefs.mode }
                : (THEME_CATALOG.find((t) => t.id === prefs.accent) ?? {
                    ...DEFAULT_OPTION,
                    type: prefs.mode,
                  })
            }
            onChange={(_, v) =>
              v.id === 'default' ? set({ accent: 'default' }) : set({ accent: v.id })
            }
            renderInput={(params) => (
              <TextField
                {...params}
                variant="outlined"
                label="Theme"
                placeholder="Search themes…"
              />
            )}
            renderOption={(props, o) => (
              <li {...props} key={o.id}>
                <Box sx={{ flexGrow: 1 }}>{o.name}</Box>
                {o.type === 'dark' ? (
                  <DarkModeIcon fontSize="small" color="action" />
                ) : (
                  <LightModeIcon fontSize="small" color="action" />
                )}
              </li>
            )}
          />
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
          <Typography variant="caption">PDF pages in a dark theme</Typography>
          <ToggleButtonGroup
            exclusive
            fullWidth
            size="small"
            sx={{ '& .MuiToggleButton-root': { flex: 1 } }}
            value={prefs.pageDim}
            onChange={(_, v) => v && set({ pageDim: v })}
            aria-label="PDF pages in a dark theme"
          >
            <ToggleButton value="off">As printed</ToggleButton>
            <ToggleButton value="dim">Dimmed</ToggleButton>
            <ToggleButton value="invert">Inverted</ToggleButton>
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
