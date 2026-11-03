'use strict';
api('state').then((s) => { $('#ver').textContent = s.version || '?'; });
