import { TextField } from '@mui/material';

interface EditorProps {
  value: string;
  onChange: (v: string) => void;
  onSave: () => void;
}

export default function Editor({ value, onChange, onSave }: EditorProps) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.ctrlKey && e.key === 's') {
      e.preventDefault();
      onSave();
    }
  };

  return (
    <TextField
      multiline
      fullWidth
      minRows={20}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={handleKeyDown}
    />
  );
}