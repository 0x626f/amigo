// Uses the same Web Crypto primitives and envelope as Finscreen's jsonPlan.ts.
// Invoked by the optional Rust interoperability test; requires Node.js.
import { webcrypto } from 'node:crypto';
const input = await new Promise((resolve, reject) => {
  let text = '';
  process.stdin.setEncoding('utf8');
  process.stdin.on('data', chunk => { text += chunk; });
  process.stdin.on('end', () => resolve(text));
  process.stdin.on('error', reject);
});
const { envelope, password } = JSON.parse(input);
const bytes = text => Uint8Array.from(Buffer.from(text, 'base64'));
const passwordKey = await webcrypto.subtle.importKey('raw', new TextEncoder().encode(password), 'PBKDF2', false, ['deriveKey']);
const key = await webcrypto.subtle.deriveKey({ name:'PBKDF2', hash:'SHA-256', salt:bytes(envelope.salt), iterations:envelope.iterations }, passwordKey, { name:'AES-GCM', length:256 }, false, ['encrypt', 'decrypt']);
const plain = await webcrypto.subtle.decrypt({ name:'AES-GCM', iv:bytes(envelope.iv) }, key, bytes(envelope.data));
const payload = JSON.parse(new TextDecoder().decode(plain));
const salt = webcrypto.getRandomValues(new Uint8Array(16));
const iv = webcrypto.getRandomValues(new Uint8Array(12));
const exportKey = await webcrypto.subtle.deriveKey({ name:'PBKDF2', hash:'SHA-256', salt, iterations:210_000 }, passwordKey, { name:'AES-GCM', length:256 }, false, ['encrypt']);
const data = await webcrypto.subtle.encrypt({ name:'AES-GCM', iv }, exportKey, new TextEncoder().encode(JSON.stringify(payload)));
process.stdout.write(JSON.stringify({ payload, envelope:{ version:1, encrypted:true, algorithm:'AES-GCM', kdf:'PBKDF2-SHA-256', iterations:210_000, salt:Buffer.from(salt).toString('base64'), iv:Buffer.from(iv).toString('base64'), data:Buffer.from(data).toString('base64') } }));
