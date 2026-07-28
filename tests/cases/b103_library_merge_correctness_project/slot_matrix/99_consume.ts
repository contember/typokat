declare const holder: console;
declare const options: Intl.B103SlotOptions;

const slot: number = holder.b103Slot;
const wrongSlot: string = holder.b103Slot; // error[TK2322]
const one: 1 = parseInt("one");
const ordinary: number = parseInt("10");
const wrongOrdinary: string = parseInt("10"); // error[TK2322]
const enabled: boolean = options.enabled;
const wrongEnabled: string = options.enabled; // error[TK2322]
