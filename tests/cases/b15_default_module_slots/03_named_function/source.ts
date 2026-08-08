export default function make(value: number): number { return value; }
const local: number = make(1);
export const localValue: number = local;
