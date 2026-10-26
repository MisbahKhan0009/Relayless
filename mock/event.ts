export type UnlistenFn = () => void;
export async function listen<T>(_e: string, _cb: (e: { payload: T }) => void): Promise<UnlistenFn> {
  return () => {};
}
