declare let longTupleDiagnosticTarget: [{ value: number }];
longTupleDiagnosticTarget = [{ extra: true }, 1]; // error[TK2353]
longTupleDiagnosticTarget = [{ value: 1 }, 1]; // error[TK2322]
longTupleDiagnosticTarget = [{ value: 1 }];

declare let shortTupleDiagnosticTarget: [{ value: number }, number];
shortTupleDiagnosticTarget = [{ extra: true }]; // error[TK2353]
shortTupleDiagnosticTarget = [{ value: 1 }]; // error[TK2322]
shortTupleDiagnosticTarget = [{ value: 1 }, 2];
