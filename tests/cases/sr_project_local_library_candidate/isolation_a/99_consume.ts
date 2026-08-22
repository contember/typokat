const ownA: number = [1].projectAOnly();
[1].projectBOnly(); // error[TK2551]: Property 'projectBOnly' does not exist on type 'number[]'. Did you mean 'projectAOnly'?
