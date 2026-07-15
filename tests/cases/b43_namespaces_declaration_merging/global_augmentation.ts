// tsc 6.0.3 --strict: TS2353 and TS2322 below; both declare-global blocks are legal here.
export {};

interface WU0Global {
  moduleOnly: boolean;
}

declare global {
  interface WU0Global {
    value: number;
  }
  interface WU0GlobalConsumer {
    target: WU0Global;
  }
}

declare global {
  interface WU0Global {
    label: string;
  }
}

const moduleLocalOk: WU0Global = { moduleOnly: true };
const moduleLocalWrong: WU0Global = { value: 1, label: "not local" }; // error[TK2353]
declare const globalConsumer: WU0GlobalConsumer;
const globalValue: number = globalConsumer.target.value;
const globalLabel: string = globalConsumer.target.label;
const globalWrong: boolean = globalConsumer.target.value; // error[TK2322]: Type 'number' is not assignable to type 'boolean'
