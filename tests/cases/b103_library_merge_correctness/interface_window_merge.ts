// Backlog 103 correctness: the canonical DOM augmentation idiom merges in one universe.
interface Window {
  b103Flag: boolean;
}

declare const view: Window;
const flag: boolean = view.b103Flag;
const wrongFlag: string = view.b103Flag; // error[TK2322]: Type 'boolean' is not assignable to type 'string'
const width: number = view.innerWidth;
const wrongWidth: string = view.innerWidth; // error[TK2322]
