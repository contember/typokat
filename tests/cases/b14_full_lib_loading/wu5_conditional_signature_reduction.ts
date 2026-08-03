// tsc 6.0.3 --strict --noEmit: only the two marked required-signature controls report TS2554.

declare const condition: boolean;

function objectRequiredUndefinedVsOptionalString(
    left: { invoke(value: string | undefined): void },
    right: { invoke(value?: string): void },
) {
    const chosen = condition ? left : right;
    chosen.invoke();
    chosen.invoke("ok");
}

function objectRequiredUndefinedVsOptionalLiteral(
    left: { invoke(value: string | undefined): void },
    right: { invoke(value?: "ok"): void },
) {
    const chosen = condition ? left : right;
    chosen.invoke();
    chosen.invoke("ok");
}

function functionRequiredUndefinedVsOptionalString(
    left: (value: string | undefined) => void,
    right: (value?: string) => void,
) {
    const chosen = condition ? left : right;
    chosen();
    chosen("ok");
}

function functionRequiredUndefinedVsOptionalLiteral(
    left: (value: string | undefined) => void,
    right: (value?: "ok") => void,
) {
    const chosen = condition ? left : right;
    chosen();
    chosen("ok");
}

function objectZeroVsOptional(
    left: { invoke(): void },
    right: { invoke(value?: string): void },
) {
    const chosen = condition ? left : right;
    chosen.invoke();
    chosen.invoke("ok");
}

function functionZeroVsOptional(
    left: () => void,
    right: (value?: string) => void,
) {
    const chosen = condition ? left : right;
    chosen();
    chosen("ok");
}

function objectRequiredVsRequired(
    left: { invoke(value: string): void },
    right: { invoke(value: "ok"): void },
) {
    const chosen = condition ? left : right;
    chosen.invoke(); // error[TK2554]
    chosen.invoke("ok");
}

function functionRequiredVsRequired(
    left: (value: string) => void,
    right: (value: "ok") => void,
) {
    const chosen = condition ? left : right;
    chosen(); // error[TK2554]
    chosen("ok");
}
