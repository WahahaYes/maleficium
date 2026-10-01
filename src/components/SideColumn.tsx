import type { ComponentProps } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import FileTree from './FileTree';
import OutlineView from './OutlineView';
import SearchPanel from './SearchPanel';

// The left column: project search while it is open, otherwise the file tree
// over the outline; a hint when no project is open.
export default function SideColumn({
  root,
  searchOpen,
  search,
  tree,
  outlineVisible,
  outline,
}: {
  root: string | null;
  searchOpen: boolean;
  search: ComponentProps<typeof SearchPanel>;
  tree: ComponentProps<typeof FileTree>;
  outlineVisible: boolean;
  outline: ComponentProps<typeof OutlineView>;
}) {
  return (
    <Box
      sx={{
        width: 260,
        flexShrink: 0,
        overflow: 'auto',
        borderRight: 1,
        borderColor: 'divider',
        p: 1,
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      {root && searchOpen ? (
        <SearchPanel {...search} />
      ) : root ? (
        <>
          <Box sx={{ flexShrink: 0 }}>
            <FileTree {...tree} />
          </Box>
          {outlineVisible ? <OutlineView {...outline} /> : null}
        </>
      ) : (
        <Typography variant="body2" color="text.secondary">
          Open a project to browse files.
        </Typography>
      )}
    </Box>
  );
}
