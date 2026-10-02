import type { components, RevisionTerms } from '@exchange/api-client';
import { isOverdue, todayIn, type Slot } from '@exchange/shared';
import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { useI18n } from '../lib/context';
import { space, useColors } from '../lib/theme';
import { Heading, Hint, Label, P, Tag, Tags, Written } from './ui';

type Contribution = components['schemas']['ContributionDto'];
type Status = components['schemas']['Status'];

interface Props {
  terms: RevisionTerms;
  currency: string;
  /** The exchange's timezone, which its due dates are read in. Unknown before joining. */
  timezone?: string;
  /** Which side the reader is; `null` for someone who has not joined yet. */
  you: Slot | null;
  /** Where each contribution stands, once the terms are in force. */
  statuses?: ReadonlyMap<string, Status>;
  /** Shown under a contribution: its status and what can be done about it. */
  footer?: (contribution: Contribution) => ReactNode;
}

/**
 * The complete terms of a revision, with nothing collapsed or left for later:
 * this is what a signature covers (DESIGN.md §14.1). Used wherever terms are
 * read, so a proposal looks the same before signing as the agreement does
 * after.
 */
export function TermsView({ terms, currency, timezone, you, statuses, footer }: Props) {
  const { wording, fmt, day, money } = useI18n();
  const colors = useColors();
  const w = wording.terms;
  const today = timezone ? todayIn(timezone) : null;
  const nameOf = (slot: Slot) => (slot === 'A' ? terms.party_a_name : terms.party_b_name);

  function due(contribution: Contribution): string {
    const condition = contribution.due;
    if (condition.kind === 'DATE') return fmt(w.dueOnDate, { date: day(condition.date) });
    if (condition.kind === 'ON_AGREEMENT') return w.dueOnAgreement;
    const awaited = terms.contributions.find((other) => other.id === condition.contribution);
    return fmt(w.dueAfter, { description: awaited?.description ?? '' });
  }

  return (
    <View style={styles.terms}>
      <Hint>{w.ownWords}</Hint>

      <Heading level={3}>{w.partiesHeading}</Heading>
      {(['A', 'B'] as const).map((slot) => (
        <Written key={slot}>
          {slot === you ? fmt(wording.party.nameYou, { name: nameOf(slot) }) : nameOf(slot)}
        </Written>
      ))}

      {terms.terms.trim() !== '' && (
        <>
          <Heading level={3}>{w.termsHeading}</Heading>
          <Written>{terms.terms}</Written>
        </>
      )}

      {(['A', 'B'] as const).map((slot) => {
        const provided = terms.contributions.filter((contribution) => contribution.from === slot);
        return (
          <View key={slot} style={styles.terms}>
            <Heading level={3}>
              {slot === you ? w.youProvide : fmt(w.otherProvides, { name: nameOf(slot) })}
            </Heading>
            {provided.length === 0 && <P>{w.nothing}</P>}
            {provided.map((contribution) => {
              const status = statuses?.get(contribution.id);
              const overdue =
                status !== undefined &&
                today !== null &&
                isOverdue(status, contribution.due, today);
              return (
                <View
                  key={contribution.id}
                  style={[styles.contribution, { borderColor: colors.border }]}>
                  <Tags>
                    <Tag>{wording.contributionTypes[contribution.type]}</Tag>
                    <Tag>{contribution.required ? w.required : w.optional}</Tag>
                    {overdue && <Tag alert>{w.overdue}</Tag>}
                  </Tags>
                  <Written>{contribution.description}</Written>
                  {contribution.amount_minor != null && (
                    <P>{fmt(w.amount, { amount: money(contribution.amount_minor, currency) })}</P>
                  )}
                  {contribution.quantity && (
                    <P>
                      {contribution.quantity.unit
                        ? fmt(w.quantityWithUnit, {
                            amount: contribution.quantity.amount,
                            unit: contribution.quantity.unit,
                          })
                        : fmt(w.quantity, { amount: contribution.quantity.amount })}
                    </P>
                  )}
                  <P>{due(contribution)}</P>
                  {contribution.completion_criteria ? (
                    <>
                      <Label>{w.criteriaLabel}</Label>
                      <Written>{contribution.completion_criteria}</Written>
                    </>
                  ) : null}
                  {footer?.(contribution)}
                </View>
              );
            })}
          </View>
        );
      })}

      {timezone && terms.contributions.some((contribution) => contribution.due.kind === 'DATE') && (
        <Hint>{fmt(w.timezone, { timezone })}</Hint>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  terms: { gap: space.m },
  contribution: { borderTopWidth: 1, paddingTop: space.m, gap: space.s },
});
