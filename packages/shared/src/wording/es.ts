import type { Wording } from './types'

export const es: Wording = {
  productName: 'Exchange',
  tagline: 'Acuerda un trato y lleva el registro de lo que cada parte ha entregado.',
  service: {
    checking: 'Comprobando el servicio…',
    connected: 'Conectado al servicio.',
    unreachable: 'No se puede conectar con el servicio en este momento.',
  },
  errors: {
    STALE_REVISION: 'Los términos cambiaron mientras los revisabas. Revisa la versión más reciente.',
    WRONG_ACTOR: 'Solo la otra parte puede hacer esto.',
    ACTION_NOT_ALLOWED: 'Esto no se puede hacer en este momento.',
    CONTRIBUTION_LOCKED: 'Este elemento ya fue aceptado y no se puede cambiar.',
    CLIENT_TOO_OLD: 'Actualiza la aplicación para continuar.',
    NOT_FOUND: 'No pudimos encontrar eso.',
    SERVICE_UNAVAILABLE: 'El servicio no está disponible temporalmente. Inténtalo de nuevo en unos momentos.',
    INTERNAL: 'Algo salió mal de nuestro lado. Inténtalo de nuevo.',
  },
}
