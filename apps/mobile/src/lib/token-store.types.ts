/**
 * Where the session token is kept between launches. On a device this is the
 * platform's secure storage and nothing else (DESIGN.md §13.1).
 */
export interface TokenStore {
  read(): Promise<string | null>;
  write(token: string): Promise<void>;
  clear(): Promise<void>;
}
