import { List, ListItemButton, ListItemIcon, Typography } from '@mui/material'
import { TreeEntry } from '../lib/files'

interface FileTreeProps {
  tree: TreeEntry[]
  selected: string | null
  onSelect: (path: string) => void
}

function FileNode({ node, depth, selected, onSelect }: {
  node: TreeEntry
  depth: number
  selected: string | null
  onSelect: (path: string) => void
}) {
  const isOpen = node.children && node.children.length > 0
  return (
    <>
      <ListItemButton selected={selected === node.path} onClick={() => onSelect(node.path)} sx={{ pl: depth * 2 }}>
        <ListItemIcon>
          <Typography variant="body2">{node.type === 'dir' ? '📁' : '📄'}</Typography>
        </ListItemIcon>
        <Typography variant="body2">{node.name}</Typography>
      </ListItemButton>
      {isOpen && node.children?.map(child => (
        <FileNode key={child.path} node={child} depth={depth + 1} selected={selected} onSelect={onSelect} />
      ))}
    </>
  )
}

export default function FileTree({ tree, selected, onSelect }: FileTreeProps) {
  return (
    <List dense>
      {tree.map(node => (
        <FileNode key={node.path} node={node} depth={0} selected={selected} onSelect={onSelect} />
      ))}
    </List>
  )
}