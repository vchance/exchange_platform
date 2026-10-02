import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import './index.css'
import { App } from './app/App.tsx'
import { deviceLanguage, loadWording } from './app/wording.ts'

// Nothing can be shown without wording, so the one language this visitor
// needs is fetched before the first render.
const language = deviceLanguage()

loadWording(language).then((wording) => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App initialLanguage={language} initialWording={wording} />
    </StrictMode>,
  )
})
