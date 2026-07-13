// tsc 6.0.3 --strict and typokat both report depth error 2589 at the final call.

type ReceiverLoop<T> = T extends string ? ReceiverLoop<T> : never;

interface RejectedGenericReceiverProbe {
    tag: "ok";
    run<T>(this: ReceiverLoop<T>, seed: T, marker: "cycle"): void;
    run(this: { tag: "ok" }, seed: string, marker: "ok"): void;
}

declare const probe: RejectedGenericReceiverProbe;
probe.run("seed", "ok"); // error[TK2589]
