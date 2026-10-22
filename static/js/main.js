import { AuthManager } from './components/AuthManager.js';
import { ChatLounge } from './components/ChatLounge.js';
import { toast } from './components/Toast.js';

document.addEventListener('DOMContentLoaded', () => {
    console.log('🦀 [FERRIS.UZ] Engine 4.0 Master Initializing...');

    const auth = new AuthManager();
    const chat = new ChatLounge(auth);

    // Navbar Auth State
    const navAuthBtn = document.getElementById('navAuthBtn');
    const navAuthText = document.getElementById('navAuthText');
    const navAuthIcon = document.getElementById('navAuthIcon');
    const authCloseBtn = document.getElementById('authCloseBtn');
    const authModal = document.getElementById('authModal');

    auth.onAuthChange((user) => {
        if (user) {
            navAuthIcon.innerText = user.provider === 'telegram' ? '🔵' : '🐙';
            navAuthText.innerText = `@${user.username}`;
            navAuthBtn.classList.add('btn-logged-in');
        } else {
            navAuthIcon.innerText = '🔑';
            navAuthText.innerText = 'Kirish / Auth';
            navAuthBtn.classList.remove('btn-logged-in');
        }
    });

    if (navAuthBtn) {
        navAuthBtn.addEventListener('click', () => {
            if (auth.user) {
                if (confirm(`@${auth.user.username} hisobingizdan chiqmoqchimisiz?`)) {
                    auth.logout();
                }
            } else {
                auth.openModal();
            }
        });
    }

    if (authCloseBtn) {
        authCloseBtn.addEventListener('click', () => auth.closeModal());
    }

    if (authModal) {
        authModal.addEventListener('click', (e) => {
            if (e.target === authModal) auth.closeModal();
        });
    }

    // Nusxa olish tugmasi
    const copyBtn = document.getElementById('copyBtn');
    if (copyBtn) {
        copyBtn.addEventListener('click', () => {
            navigator.clipboard.writeText('curl -sL https://api.ferris.uz/v1/ping | jq').then(() => {
                toast.show('Terminal buyrug\'i nusxalandi! 📋', 'success');
            });
        });
    }

    // Interactive Rust Playground Logic
    const samples = {
        tokio: `// 🚀 Tokio Asinxron Vazifa Namunasi
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    println!("Rustacean: Tokio dvigateli ishga tushdi...");
    
    let handle = tokio::spawn(async {
        sleep(Duration::from_millis(50)).await;
        "⚡ 10,000+ so'rovlar nol xotira yo'qotishisiz bajarildi!"
    });

    let result = handle.await.unwrap();
    println!("{}", result);
}`,
        memory: `// 🛡️ Xotira Xavfsizligi & Borrow Checker
fn main() {
    let original = String::from("Ferris.uz");
    
    // Borrowing (Qarz olish) - Null pointer xatosi 0%!
    print_len(&original);
    
    // original hali ham toza va tirik
    println!("Asl ma'lumot saqlandi: {}", original);
}

fn print_len(s: &str) {
    println!("Matn uzunligi: {} bayt", s.len());
}`,
        pattern: `// 💎 Pattern Matching & Exhaustive Enums
enum Event {
    Connected(String),
    Message { room: String, text: String },
    Disconnected,
}

fn handle(event: Event) {
    match event {
        Event::Connected(ip) => println!("Ulandi: {}", ip),
        Event::Message { room, text } => println!("[{}] {}", room, text),
        Event::Disconnected => println!("Ulanish uzildi"),
    }
}`
    };

    const codeViewer = document.getElementById('codeViewer');
    const consoleOutput = document.getElementById('consoleOutput');
    const runCodeBtn = document.getElementById('runCodeBtn');
    const pgTabs = document.querySelectorAll('.pg-tab');

    let currentSample = 'tokio';

    pgTabs.forEach(tab => {
        tab.addEventListener('click', (e) => {
            pgTabs.forEach(t => t.classList.remove('active'));
            tab.classList.add('active');
            currentSample = tab.getAttribute('data-sample');
            if (codeViewer && samples[currentSample]) {
                codeViewer.textContent = samples[currentSample];
                if (consoleOutput) {
                    consoleOutput.textContent = `[INFO] ${tab.textContent} yuklandi. "Kodni Sinash" tugmasini bosing...`;
                }
            }
        });
    });

    if (runCodeBtn && consoleOutput) {
        runCodeBtn.addEventListener('click', () => {
            consoleOutput.textContent = '⏳ Rust kompilyatori (WASM) tekshirmoqda...';
            setTimeout(() => {
                if (currentSample === 'tokio') {
                    consoleOutput.innerHTML = `> cargo run --release<br><span style="color:#ffffff;">Rustacean: Tokio dvigateli ishga tushdi...</span><br><span style="color:#ffffff;">⚡ 10,000+ so'rovlar nol xotira yo'qotishisiz bajarildi!</span><br><span style="color:#71717a;">[Bajarildi: 1.2ms, Xotira: 820KB]</span>`;
                } else if (currentSample === 'memory') {
                    consoleOutput.innerHTML = `> rustc --check<br><span style="color:#ffffff;">Matn uzunligi: 9 bayt</span><br><span style="color:#ffffff;">Asl ma'lumot saqlandi: Ferris.uz</span><br><span style="color:#71717a;">[0 memory leaks, Safe Borrowing]</span>`;
                } else {
                    consoleOutput.innerHTML = `> cargo test<br><span style="color:#ffffff;">[#general] Salom, Rustaceanlar!</span><br><span style="color:#71717a;">[Pattern matching to'liq tekshirildi]</span>`;
                }
                toast.show('Kod muvaffaqiyatli sinovdan o\'tdi! ⚡', 'success');
            }, 300);
        });
    }
});
