// Backlog 70 — represented method and object call signatures retain their
// explicit receiver and diagnose incompatible call-site this contexts.

interface GoodMethodReceiver {
  n: number;
  run(this: { n: number }, value: string): void;
}

declare const goodMethod: GoodMethodReceiver;
goodMethod.run("ok");

interface BadMethodReceiver {
  n: string;
  run(this: { n: number }, value: string): void;
}

declare const badMethod: BadMethodReceiver;
badMethod.run("bad"); // error[TK2684]

type CallableWithReceiver = {
  (this: { n: number }, value: string): void;
};

declare const callable: CallableWithReceiver;
callable("bad"); // error[TK2684]
