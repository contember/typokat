export function local(value: number): number;
export function local(value: string): string;
export default function local(value: number | string): number | string {
  return value;
}
