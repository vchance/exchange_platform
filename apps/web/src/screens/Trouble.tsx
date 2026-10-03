import type { ExchangeView } from '@yuppers/api-client'
import {
  moveWording,
  TROUBLE_SITUATIONS,
  troubleRoute,
  troubleSituationOf,
  type TroubleOffer,
  type TroubleSituation,
  type TroubleWay,
  type Wording,
} from '@yuppers/shared'
import { useEffect, useId, useRef, useState } from 'react'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { Panel } from '../components/Panel'
import { Written } from '../components/ui'
import type { Actions } from '../lib/actions'

interface Props {
  exchange: ExchangeView
  otherName: string
  actions: Actions
}

/** What each way forward is called on its button: the action's own name. */
function wayLabel(wording: Wording, way: TroubleWay, money: boolean): string {
  switch (way) {
    case 'DISPUTE':
    case 'WAIVE':
    case 'RECLAIM':
      return moveWording(wording, way, money)
    case 'PROPOSE_END':
      return wording.exchange.proposeEnd
    case 'AGREE_END':
      return wording.exchange.agreeEnd
    case 'REQUEST_CLOSE':
      return wording.exchange.requestClose
    case 'AMEND':
      return wording.exchange.amend
  }
}

/**
 * "Something isn't working" (DESIGN.md §5.3): asks what the situation is,
 * says what each way forward means for who is released from what, and opens
 * the existing action the person picks, where they confirm it as usual. It
 * sends nothing itself.
 */
export function Trouble({ exchange, otherName, actions }: Props) {
  const { wording, fmt } = useI18n()
  const t = wording.trouble
  const started = troubleSituationOf(actions.panel)
  const [situation, setSituation] = useState<TroubleSituation | null>(started ?? null)
  const explained = useRef<HTMLParagraphElement>(null)
  const question = useId()

  // A situation chosen here puts the keyboard on what it says; one the
  // panel opened at is reached with the panel itself.
  const [chosen, setChosen] = useState(false)
  useEffect(() => {
    if (chosen && situation) explained.current?.focus()
  }, [chosen, situation])

  const route = situation ? troubleRoute(exchange, situation) : null

  return (
    <Panel title={t.open}>
      {!situation || !route ? (
        <>
          <p>{t.intro}</p>
          <p className="label" id={question}>
            {t.question}
          </p>
          <ul className="plain trouble-choices" aria-labelledby={question}>
            {TROUBLE_SITUATIONS.map((option) => (
              <li key={option}>
                <button
                  type="button"
                  onClick={() => {
                    setChosen(true)
                    setSituation(option)
                  }}
                >
                  {fmt(t.situations[option], { name: otherName })}
                </button>
              </li>
            ))}
          </ul>
        </>
      ) : (
        <>
          <h3 className="trouble-situation">{fmt(t.situations[situation], { name: otherName })}</h3>
          <p tabIndex={-1} ref={explained}>
            {fmt(t.explain[situation], { name: otherName })}
          </p>
          {route.notes.map((note) => (
            <p key={note} className="notice">
              {fmt(t[note], { name: otherName })}
            </p>
          ))}
          {route.offers.map((offer) => (
            <Way key={offer.way} offer={offer} exchange={exchange} otherName={otherName} actions={actions} />
          ))}
        </>
      )}
      <div className="actions">
        {situation && (
          <button
            type="button"
            className="link"
            onClick={() => {
              setChosen(false)
              setSituation(null)
            }}
          >
            {t.change}
          </button>
        )}
        <button type="button" onClick={actions.close}>
          {wording.common.cancel}
        </button>
      </div>
    </Panel>
  )
}

interface WayProps {
  offer: TroubleOffer
  exchange: ExchangeView
  otherName: string
  actions: Actions
}

/** One way forward: what it means, then the action, on each item it can be taken on. */
function Way({ offer, exchange, otherName, actions }: WayProps) {
  const { wording, fmt } = useI18n()
  const means = fmt(wording.trouble.means[offer.way], { name: otherName })
  const base = useId()

  if (offer.items.length > 0) {
    return (
      <div className="trouble-way">
        <p>{means}</p>
        <ul className="plain">
          {offer.items.map((item) => (
            <li key={item.id} className="trouble-item">
              <div id={`${base}-${item.id}`}>
                <Written>{item.description}</Written>
              </div>
              <button
                type="button"
                aria-describedby={`${base}-${item.id}`}
                disabled={actions.busy}
                onClick={() => actions.open(item.panel)}
              >
                {wayLabel(wording, offer.way, item.money)}
              </button>
            </li>
          ))}
        </ul>
      </div>
    )
  }
  return (
    <div className="trouble-way">
      <p>{means}</p>
      <div className="actions">
        {offer.panel ? (
          <button
            type="button"
            disabled={actions.busy}
            onClick={() => actions.open(offer.panel!)}
          >
            {wayLabel(wording, offer.way, false)}
          </button>
        ) : (
          <Link className="button" to={paths.revise(exchange.id)}>
            {wayLabel(wording, offer.way, false)}
          </Link>
        )}
      </div>
    </div>
  )
}
