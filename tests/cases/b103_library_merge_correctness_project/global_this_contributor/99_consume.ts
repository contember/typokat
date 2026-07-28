const enabled: boolean = globalThis.B103GlobalThisValue.enabled;
const wrongEnabled: string = globalThis.B103GlobalThisValue.enabled; // error[TK2322]
const called: number = globalThis.B103GlobalThisCall();
const wrongCalled: string = globalThis.B103GlobalThisCall(); // error[TK2322]
