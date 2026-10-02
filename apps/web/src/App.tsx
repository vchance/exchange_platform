import { createApiClient } from '@exchange/api-client'
import { directionOf, pickLanguage, wordingFor } from '@exchange/shared'
import { useEffect, useState } from 'react'

// Same origin: the dev server proxies API paths to the Rust service.
const api = createApiClient('')

type ServiceState = 'checking' | 'connected' | 'unreachable'

function App() {
  const language = pickLanguage(navigator.languages)
  const wording = wordingFor(language)
  const [service, setService] = useState<ServiceState>('checking')

  useEffect(() => {
    document.documentElement.lang = language
    document.documentElement.dir = directionOf(language)
  }, [language])

  useEffect(() => {
    let cancelled = false
    api
      .GET('/v1/meta')
      .then(({ data }) => {
        if (!cancelled) setService(data ? 'connected' : 'unreachable')
      })
      .catch(() => {
        if (!cancelled) setService('unreachable')
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <main>
      <h1>{wording.productName}</h1>
      <p>{wording.tagline}</p>
      <p role="status">{wording.service[service]}</p>
    </main>
  )
}

export default App
