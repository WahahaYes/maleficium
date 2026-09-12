import Box from '@mui/material/Box'

export default function MainLayout({editor, preview}:{editor:React.ReactNode, preview:React.ReactNode}) {
  return (
    <Box sx={{display: 'flex', height: '100vh'}}>
      <Box sx={{flex: 1, overflow: 'auto'}}>
        {editor}
      </Box>
      <Box sx={{flex: 1, overflow: 'auto'}} id="preview-slot">
        {preview}
      </Box>
    </Box>
  )
}