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
    REVISION_EXPIRED: 'Esta oferta ha vencido. Pide una nueva o envía la tuya.',
    COUNTERPARTY_NOT_CONFIRMED: 'Confirma quién es la otra parte antes de aceptar.',
    INVALID_REVISION: 'Hay que corregir algunos términos antes de enviar.',
    INVALID_REQUEST: 'Algo en la solicitud no es correcto. Revísalo e inténtalo de nuevo.',
    INVALID_IDENTIFIER: 'Escribe un correo electrónico válido o un número de teléfono con su código de país.',
    INVALID_CODE: 'Ese código no funcionó. Revísalo o pide uno nuevo.',
    TOO_MANY_REQUESTS: 'Se pidieron demasiados códigos. Espera un rato antes de pedir otro.',
    UNAUTHENTICATED: 'Inicia sesión para continuar.',
    ACCOUNT_SUSPENDED: 'Esta cuenta ha sido suspendida.',
    IDENTIFIER_IN_USE: 'Ese correo electrónico o número de teléfono pertenece a otra cuenta.',
    CLIENT_TOO_OLD: 'Actualiza la aplicación para continuar.',
    NOT_FOUND: 'No pudimos encontrar eso.',
    SERVICE_UNAVAILABLE: 'El servicio no está disponible temporalmente. Inténtalo de nuevo en unos momentos.',
    INTERNAL: 'Algo salió mal de nuestro lado. Inténtalo de nuevo.',
  },
}
