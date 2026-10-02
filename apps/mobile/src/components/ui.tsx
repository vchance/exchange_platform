import type { ErrorCode } from '@exchange/api-client';
import { useEffect, useRef, type ReactNode, type Ref } from 'react';
import {
  AccessibilityInfo,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  RefreshControl,
  ScrollView,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  View,
  type StyleProp,
  type TextInputProps,
  type TextStyle,
  type ViewStyle,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useI18n } from '../lib/context';
import { space, TOUCH_TARGET, type, useColors } from '../lib/theme';

/*
 * The app's few building blocks: React Native's own components with the
 * system font and the platform's controls, and no UI kit. Each carries its
 * accessibility role and label, and nothing interactive is smaller than a
 * comfortable touch target.
 *
 * Nothing is positioned on the assumption that text runs left to right
 * (DESIGN.md §4.2): spacing and borders use `start` and `end`, and rows
 * follow the layout direction.
 */

/** Says something to a screen reader without moving its focus. */
function announce(text: string) {
  try {
    AccessibilityInfo.announceForAccessibility(text);
  } catch {
    // Not every platform can; what was to be announced is on screen regardless.
  }
}

interface ScreenProps {
  children: ReactNode;
  /** Pulling down refreshes, where there is something to refresh. */
  onRefresh?: () => void;
  refreshing?: boolean;
  scroll?: Ref<ScrollView>;
}

/** A screen's scrolling body, clear of the keyboard and the device's edges. */
export function Screen({ children, onRefresh, refreshing = false, scroll }: ScreenProps) {
  const colors = useColors();
  const insets = useSafeAreaInsets();
  return (
    <KeyboardAvoidingView
      style={[styles.fill, { backgroundColor: colors.background }]}
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      keyboardVerticalOffset={Platform.OS === 'ios' ? 64 : 0}>
      <ScrollView
        ref={scroll}
        style={styles.fill}
        contentInsetAdjustmentBehavior="automatic"
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={[
          styles.screen,
          {
            paddingBottom: space.xl + insets.bottom,
            paddingStart: space.l + insets.left,
            paddingEnd: space.l + insets.right,
          },
        ]}
        refreshControl={
          onRefresh ? (
            <RefreshControl
              refreshing={refreshing}
              onRefresh={onRefresh}
              tintColor={colors.muted}
            />
          ) : undefined
        }>
        <View style={styles.column}>{children}</View>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

/** A heading, announced as one. Level 1 names the screen. */
export function Heading({ children, level = 1 }: { children: string; level?: 1 | 2 | 3 }) {
  const colors = useColors();
  const size = level === 1 ? type.title : level === 2 ? type.heading : type.subheading;
  return (
    <Text
      accessibilityRole="header"
      aria-level={level}
      style={[size, styles.heading, { color: colors.text }]}>
      {children}
    </Text>
  );
}

/** A paragraph of the product's own words. */
export function P({ children, style }: { children: ReactNode; style?: StyleProp<TextStyle> }) {
  const colors = useColors();
  return <Text style={[type.body, { color: colors.text }, style]}>{children}</Text>;
}

/** Secondary text: a hint under a label, a reference, a date. */
export function Hint({ children }: { children: ReactNode }) {
  const colors = useColors();
  return <Text style={[type.hint, { color: colors.muted }]}>{children}</Text>;
}

/** A small label over something, such as "Done when". */
export function Label({ children }: { children: string }) {
  const colors = useColors();
  return <Text style={[type.body, styles.label, { color: colors.text }]}>{children}</Text>;
}

interface ButtonProps {
  label: string;
  onPress(): void;
  variant?: 'primary' | 'default' | 'link';
  disabled?: boolean;
  /** For a button that opens a panel under it: whether the panel is open. */
  expanded?: boolean;
  accessibilityLabel?: string;
  testID?: string;
}

export function Button({
  label,
  onPress,
  variant = 'default',
  disabled = false,
  expanded,
  accessibilityLabel,
  testID,
}: ButtonProps) {
  const colors = useColors();
  const primary = variant === 'primary';
  const link = variant === 'link';
  return (
    <Pressable
      testID={testID}
      accessibilityRole={link ? 'link' : 'button'}
      accessibilityLabel={accessibilityLabel ?? label}
      // Said both ways: the first is what the platforms read, the second what a browser does.
      accessibilityState={{ disabled, expanded }}
      aria-disabled={disabled || undefined}
      aria-expanded={expanded}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        styles.button,
        link
          ? styles.buttonLink
          : {
              borderColor: primary ? colors.primary : colors.border,
              backgroundColor: primary ? colors.primary : colors.background,
              borderWidth: 1,
            },
        (pressed || disabled) && styles.dimmed,
      ]}>
      <Text
        style={[
          type.body,
          styles.buttonText,
          { color: primary ? colors.onPrimary : link ? colors.primary : colors.text },
          link && styles.underlined,
        ]}>
        {label}
      </Text>
    </Pressable>
  );
}

/** A group of buttons. It wraps, so a long translation never needs a fixed width. */
export function Actions({ children }: { children: ReactNode }) {
  return <View style={styles.actions}>{children}</View>;
}

interface NoteProps {
  children: ReactNode;
  tone?: 'info' | 'warning' | 'error';
  /** The text to read out when it appears. */
  spoken?: string;
}

function Note({ children, tone = 'info', spoken }: NoteProps) {
  const colors = useColors();
  const error = tone === 'error';
  useEffect(() => {
    if (spoken) announce(spoken);
  }, [spoken]);
  return (
    <View
      accessibilityRole={error ? 'alert' : undefined}
      accessibilityLiveRegion={error ? 'assertive' : 'polite'}
      style={[
        styles.note,
        {
          borderStartColor: error ? colors.danger : colors.primary,
          backgroundColor: error
            ? colors.dangerSurface
            : tone === 'warning'
              ? colors.warningSurface
              : colors.noticeSurface,
        },
      ]}>
      {typeof children === 'string' ? <P>{children}</P> : children}
    </View>
  );
}

/** A refusal from the service, in the reader's language. Announced when it appears. */
export function Failure({ code }: { code: ErrorCode | null }) {
  const { errorText } = useI18n();
  if (!code) return null;
  return <ErrorNote>{errorText(code)}</ErrorNote>;
}

/** Something that went wrong, said in the product's own wording. */
export function ErrorNote({ children }: { children: string }) {
  return (
    <Note tone="error" spoken={children}>
      {children}
    </Note>
  );
}

/** Something that happened, announced without interrupting. */
export function Notice({
  children,
  tone,
  quiet,
}: {
  children: ReactNode;
  tone?: 'info' | 'warning';
  /** Shown but not read out by itself: standing information, not news. */
  quiet?: boolean;
}) {
  return (
    <Note tone={tone} spoken={!quiet && typeof children === 'string' ? children : undefined}>
      {children}
    </Note>
  );
}

interface FieldProps {
  label: string;
  hint?: string;
  error?: string | null;
  /**
   * The control says the label and hint to a screen reader itself, so the
   * words above it are for the eye only and are not read a second time.
   */
  labelled?: boolean;
  children: ReactNode;
}

/** A label, its hint and its error around a control. */
export function Field({ label, hint, error, labelled = false, children }: FieldProps) {
  const colors = useColors();
  return (
    <View style={styles.field}>
      <View aria-hidden={labelled || undefined}>
        <Text style={[type.body, styles.label, { color: colors.text }]}>{label}</Text>
        {hint ? <Hint>{hint}</Hint> : null}
      </View>
      {children}
      {error ? (
        <Text
          accessibilityRole="alert"
          accessibilityLiveRegion="polite"
          style={[type.hint, styles.fieldError, { color: colors.danger }]}>
          {error}
        </Text>
      ) : null}
    </View>
  );
}

interface TextFieldProps extends Omit<TextInputProps, 'style' | 'editable'> {
  label: string;
  hint?: string;
  error?: string | null;
  disabled?: boolean;
  input?: Ref<TextInput>;
}

/** A labelled text input. Text is laid out in whichever direction its own script runs. */
export function TextField({ label, hint, error, disabled, input, ...rest }: TextFieldProps) {
  const colors = useColors();
  return (
    <Field label={label} hint={hint} error={error} labelled>
      <TextInput
        ref={input}
        accessibilityLabel={label}
        accessibilityHint={[hint, error].filter(Boolean).join(' ') || undefined}
        aria-invalid={error ? true : undefined}
        editable={!disabled}
        placeholderTextColor={colors.muted}
        {...rest}
        style={[
          type.body,
          styles.input,
          rest.multiline && styles.inputMultiline,
          {
            color: colors.text,
            borderColor: error ? colors.danger : colors.border,
            backgroundColor: colors.background,
          },
          disabled && styles.dimmed,
        ]}
      />
    </Field>
  );
}

interface ChoiceProps<T extends string> {
  label: string;
  hint?: string;
  error?: string | null;
  value: T | null;
  options: readonly { value: T; label: string }[];
  onChange(value: T): void;
  disabled?: boolean;
}

/** One of a few options, as a group of radio buttons. */
export function Choice<T extends string>({
  label,
  hint,
  error,
  value,
  options,
  onChange,
  disabled = false,
}: ChoiceProps<T>) {
  const colors = useColors();
  return (
    <Field label={label} hint={hint} error={error}>
      <View accessibilityRole="radiogroup" accessibilityLabel={label} style={styles.choice}>
        {options.map((option) => {
          const selected = option.value === value;
          return (
            <Pressable
              key={option.value}
              accessibilityRole="radio"
              accessibilityLabel={option.label}
              accessibilityState={{ checked: selected, disabled }}
              aria-checked={selected}
              aria-disabled={disabled || undefined}
              disabled={disabled}
              onPress={() => onChange(option.value)}
              style={({ pressed }) => [styles.option, (pressed || disabled) && styles.dimmed]}>
              <View style={[styles.radio, { borderColor: selected ? colors.primary : colors.border }]}>
                {selected ? (
                  <View style={[styles.radioDot, { backgroundColor: colors.primary }]} />
                ) : null}
              </View>
              <Text style={[type.body, styles.optionText, { color: colors.text }]}>
                {option.label}
              </Text>
            </Pressable>
          );
        })}
      </View>
    </Field>
  );
}

interface CheckProps {
  label: string;
  value: boolean;
  onChange(value: boolean): void;
  error?: string | null;
  disabled?: boolean;
  testID?: string;
}

/** A yes-or-no answer, as the platform's own switch. It is off until the person turns it on. */
export function Check({ label, value, onChange, error, disabled, testID }: CheckProps) {
  const colors = useColors();
  return (
    <View style={styles.field}>
      <View style={styles.check}>
        <Switch
          testID={testID}
          accessibilityLabel={label}
          accessibilityHint={error ?? undefined}
          value={value}
          onValueChange={onChange}
          disabled={disabled}
        />
        <Text
          // The switch is labelled; pressing the words works too, for sighted users.
          accessible={false}
          importantForAccessibility="no"
          aria-hidden
          onPress={disabled ? undefined : () => onChange(!value)}
          style={[type.body, styles.checkLabel, { color: colors.text }]}>
          {label}
        </Text>
      </View>
      {error ? (
        <Text
          accessibilityRole="alert"
          accessibilityLiveRegion="polite"
          style={[type.hint, styles.fieldError, { color: colors.danger }]}>
          {error}
        </Text>
      ) : null}
    </View>
  );
}

/** Something set apart: an exchange in a list, a proposal, the agreement. */
export function Card({ children, style }: { children: ReactNode; style?: StyleProp<ViewStyle> }) {
  const colors = useColors();
  return (
    <View
      style={[styles.card, { borderColor: colors.border, backgroundColor: colors.surface }, style]}>
      {children}
    </View>
  );
}

export function Tags({ children }: { children: ReactNode }) {
  return <View style={styles.tags}>{children}</View>;
}

export function Tag({ children, alert }: { children: string; alert?: boolean }) {
  const colors = useColors();
  return (
    <Text
      style={[
        type.hint,
        styles.tag,
        {
          color: alert ? colors.danger : colors.text,
          borderColor: alert ? colors.danger : colors.border,
          backgroundColor: colors.background,
        },
        alert && styles.strong,
      ]}>
      {children}
    </Text>
  );
}

/**
 * An action that has been opened but not sent. It appears in place, under
 * the thing it acts on, and a screen reader is taken to it.
 */
export function Panel({ title, children }: { title: string; children: ReactNode }) {
  const colors = useColors();
  const heading = useRef<Text>(null);

  useEffect(() => {
    if (Platform.OS === 'web' || !heading.current) return;
    try {
      AccessibilityInfo.sendAccessibilityEvent(heading.current, 'focus');
    } catch {
      // The panel is still there to be found.
    }
  }, []);

  return (
    <View
      accessibilityLabel={title}
      style={[styles.panel, { borderColor: colors.primary, backgroundColor: colors.background }]}>
      <Text
        ref={heading}
        accessibilityRole="header"
        style={[type.subheading, { color: colors.text }]}>
        {title}
      </Text>
      {children}
    </View>
  );
}

/**
 * Text a person wrote: a name, terms, a description, a note. It is shown
 * exactly as written and never translated (DESIGN.md §4.2), and set apart so
 * it cannot be mistaken for the product speaking. The system lays it out in
 * whichever direction its own script runs.
 */
export function Written({ children }: { children: string }) {
  const colors = useColors();
  return (
    <Text
      style={[type.body, styles.written, { color: colors.text, borderStartColor: colors.border }]}>
      {children}
    </Text>
  );
}

/** A list of short statements, each on its own line. */
export function Lines({ children }: { children: ReactNode }) {
  return <View style={styles.lines}>{children}</View>;
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  screen: { paddingTop: space.l, flexGrow: 1 },
  // Readable line length on a tablet; the full width on a phone.
  column: { width: '100%', maxWidth: 640, alignSelf: 'center', gap: space.l },
  heading: { marginTop: space.xs },
  label: { fontWeight: '600' },
  button: {
    minHeight: TOUCH_TARGET,
    minWidth: TOUCH_TARGET,
    paddingVertical: space.m,
    paddingHorizontal: space.l,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  buttonLink: { paddingHorizontal: space.xs },
  buttonText: { fontWeight: '600', textAlign: 'center' },
  underlined: { textDecorationLine: 'underline' },
  dimmed: { opacity: 0.5 },
  actions: { flexDirection: 'row', flexWrap: 'wrap', gap: space.m, alignItems: 'center' },
  note: { borderStartWidth: 4, borderRadius: 4, padding: space.m, gap: space.s },
  field: { gap: space.s },
  fieldError: { fontWeight: '600' },
  input: {
    minHeight: TOUCH_TARGET,
    borderWidth: 1,
    borderRadius: 8,
    paddingVertical: space.m,
    paddingHorizontal: space.m,
  },
  inputMultiline: { minHeight: TOUCH_TARGET * 2, textAlignVertical: 'top' },
  choice: { gap: space.xs },
  option: {
    minHeight: TOUCH_TARGET,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.m,
    paddingVertical: space.s,
  },
  optionText: { flex: 1 },
  radio: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  radioDot: { width: 12, height: 12, borderRadius: 6 },
  check: { minHeight: TOUCH_TARGET, flexDirection: 'row', alignItems: 'center', gap: space.m },
  checkLabel: { flex: 1 },
  card: { borderWidth: 1, borderRadius: 12, padding: space.l, gap: space.m },
  tags: { flexDirection: 'row', flexWrap: 'wrap', gap: space.s },
  tag: {
    borderWidth: 1,
    borderRadius: 6,
    paddingVertical: space.xs,
    paddingHorizontal: space.s,
    overflow: 'hidden',
  },
  strong: { fontWeight: '700' },
  panel: { borderWidth: 2, borderRadius: 12, padding: space.l, gap: space.m },
  written: { borderStartWidth: 3, paddingStart: space.m },
  lines: { gap: space.xs },
});
