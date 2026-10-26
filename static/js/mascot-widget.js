/**
 * FERRIS.UZ — MINIMALIST DEV COMPANION ENGINE (v4.0)
 * Designed for Vercel/Linear Monochrome aesthetic.
 * Features state persistence (localStorage), discrete speech, and refined micro-interactions.
 */

(function () {
    const STORAGE_KEY = 'ferris_companion_v4_state';
    const isFile = window.location.protocol === 'file:';
    const MASCOT_BASE = isFile ? './mascot/' : '/mascot/';

    const POSES = {
        main: MASCOT_BASE + 'ferris_main.png',
        welcome: MASCOT_BASE + 'ferris_main.png',
        coding: MASCOT_BASE + 'ferris_coding.png',
        tea: MASCOT_BASE + 'ferris_tea.png',
        rocket: MASCOT_BASE + 'ferris_rocket.png',
        shield: MASCOT_BASE + 'ferris_shield.png',
        success: MASCOT_BASE + 'ferris_success.png',
        error: MASCOT_BASE + 'ferris_404.png',
    };

    // Concise, insightful technical and community tips:
    const DEV_TIPS = [
        { tag: 'Memory Safety', pose: 'shield', text: "Rust'da 'Null Pointer Exception' yo'q: Option<T> turlari orqali bo'sh qiymatlar xavfsiz boshqariladi." },
        { tag: 'Borrow Checker', pose: 'coding', text: "Aliasing XOR Mutability: bir vaqtning o'zida yoki bir nechta o'qish, yoki faqat 1 ta yozish ruxsat etiladi." },
        { tag: 'Zero-Cost', pose: 'rocket', text: "Zero-cost abstractions: Generics va iterators kompilatsiyada to'liq unrolling qilinib C tezligini beradi." },
        { tag: 'Error Handling', pose: 'coding', text: "try-catch o'rniga Result<T, E> va '?' operatori: xatoliklar aniq va kompilyatorda tekshiriladi." },
        { tag: 'Concurrency', pose: 'shield', text: "Send va Sync traitlari: ma'lumotlar oqimlar (threads) orasida xavfsiz almashishini kafolatlaydi." },
        { tag: 'Clippy', pose: 'coding', text: "Kodingizni tekshirish uchun: `cargo clippy --all-targets -- -D warnings` buyrug'idan foydalaning." },
        { tag: 'Optimizatsiya', pose: 'rocket', text: "Release profilida LTO (Link-Time Optimization) va codegen-units = 1 bilan maksimal RPS ga erishing." },
        { tag: 'Choyxona', pose: 'tea', text: "Bir piyola ko'k choy ichib oling. Kuchli arxitektura tinch va xotirjam aql bilan quriladi ☕" },
        { tag: 'Pattern Match', pose: 'main', text: "Rust'dagi `match` to'liq (exhaustive) bo'lishi shart — birorta holat e'tibordan chetda qolmaydi." },
        { tag: 'SQLx', pose: 'coding', text: "`sqlx` so'rovlaringizni to'g'ridan-to'g'ri kompilatsiya paytida DB sxemasi bilan sinxron tekshiradi." },
        { tag: 'doc.ferris.uz', pose: 'main', text: "doc.ferris.uz portalida rasmiy 'The Rust Programming Language' kitobi o'zbek tilida mavjud." },
        { tag: 'ferris-cli', pose: 'rocket', text: "Rasmiy CLI vositasi v0.3.0: Linux Landlock xavfsizlik enklavi va sirli tokenlar himoyachisi." },
        { tag: 'Hamjamiyat', pose: 'welcome', text: "Ferris.uz — O'zbekiston tizimli dasturchilari va Rustaceanlari rasmiy ekotizimi 🦀🇺🇿" }
    ];

    class FerrisDevCompanion {
        constructor() {
            this.state = this.loadState();
            this.currentPose = 'main';
            this.currentIndex = 0;
            this.bubbleTimeout = null;
            this.init();
        }

        loadState() {
            try {
                const saved = localStorage.getItem(STORAGE_KEY);
                if (saved) return JSON.parse(saved);
            } catch (e) {}
            return { isDocked: false, soundEnabled: false };
        }

        saveState() {
            try {
                localStorage.setItem(STORAGE_KEY, JSON.stringify(this.state));
            } catch (e) {}
        }

        init() {
            this.injectStyles();
            this.createDOM();
            this.applyInitialState();
            this.bindEvents();
        }

        injectStyles() {
            if (!document.getElementById('ferris-mascot-css')) {
                const link = document.createElement('link');
                link.id = 'ferris-mascot-css';
                link.rel = 'stylesheet';
                link.href = isFile ? './mascot/mascot-widget.css' : '/mascot/mascot-widget.css';
                document.head.appendChild(link);
            }
        }

        createDOM() {
            if (document.getElementById('ferrisMascot')) {
                document.getElementById('ferrisMascot').remove();
            }

            const container = document.createElement('div');
            container.className = 'ferris-mascot-container';
            container.id = 'ferrisMascot';

            container.innerHTML = `
                <div class="ferris-bubble" id="ferrisBubble">
                    <div class="ferris-bubble-header">
                        <div class="ferris-bubble-meta">
                            <span class="ferris-bubble-dot"></span>
                            <span class="ferris-bubble-title">Ferris</span>
                            <span class="ferris-badge-tag" id="ferrisTag">Rust</span>
                        </div>
                        <button class="ferris-bubble-close" id="ferrisCloseBtn" title="Yopish">&times;</button>
                    </div>
                    <div class="ferris-bubble-text" id="ferrisText">...</div>
                    <div class="ferris-bubble-actions">
                        <button class="ferris-btn ferris-btn-accent" id="ferrisNextTipBtn">Yangi fikr &rarr;</button>
                        <button class="ferris-btn" id="ferrisDockBtn">Yashirish</button>
                    </div>
                </div>
                <div class="ferris-avatar-box" id="ferrisAvatarBox" title="Ferris maslahatchi (bosing)">
                    <img src="${POSES.main}" alt="Ferris Mascot" class="ferris-avatar-img" id="ferrisImg" />
                    <div class="ferris-dock-indicator" id="ferrisDockIndicator" title="Ko'rsatish">FERRIS</div>
                </div>
            `;

            document.body.appendChild(container);

            this.container = container;
            this.bubble = document.getElementById('ferrisBubble');
            this.bubbleText = document.getElementById('ferrisText');
            this.tagEl = document.getElementById('ferrisTag');
            this.avatarBox = document.getElementById('ferrisAvatarBox');
            this.img = document.getElementById('ferrisImg');
            this.dockIndicator = document.getElementById('ferrisDockIndicator');
        }

        applyInitialState() {
            if (this.state.isDocked) {
                this.container.classList.add('docked');
            }
        }

        bindEvents() {
            // Click on avatar
            this.avatarBox.addEventListener('click', (e) => {
                e.stopPropagation();
                if (this.state.isDocked) {
                    this.undock();
                } else {
                    this.pulse();
                    this.nextTip();
                }
            });

            // Dock indicator click
            this.dockIndicator.addEventListener('click', (e) => {
                e.stopPropagation();
                this.undock();
            });

            // Close bubble
            document.getElementById('ferrisCloseBtn').addEventListener('click', (e) => {
                e.stopPropagation();
                this.hideBubble();
            });

            // Next tip button
            document.getElementById('ferrisNextTipBtn').addEventListener('click', (e) => {
                e.stopPropagation();
                this.nextTip();
            });

            // Dock button
            document.getElementById('ferrisDockBtn').addEventListener('click', (e) => {
                e.stopPropagation();
                this.dock();
            });
        }

        setPose(poseName) {
            if (!POSES[poseName] || this.currentPose === poseName) return;
            this.currentPose = poseName;
            this.img.style.opacity = '0';
            setTimeout(() => {
                this.img.src = POSES[poseName];
                this.img.style.opacity = '1';
            }, 120);
        }

        speak(text, tag = 'Rust', pose = 'main', duration = 8000) {
            this.setPose(pose);
            this.tagEl.textContent = tag;
            this.bubbleText.textContent = text;
            this.bubble.classList.add('visible');

            clearTimeout(this.bubbleTimeout);
            if (duration > 0) {
                this.bubbleTimeout = setTimeout(() => {
                    this.hideBubble();
                }, duration);
            }
        }

        hideBubble() {
            this.bubble.classList.remove('visible');
        }

        pulse() {
            this.avatarBox.classList.remove('ferris-pulse');
            void this.avatarBox.offsetWidth;
            this.avatarBox.classList.add('ferris-pulse');
        }

        nextTip() {
            const tip = DEV_TIPS[this.currentIndex];
            this.currentIndex = (this.currentIndex + 1) % DEV_TIPS.length;
            this.speak(tip.text, tip.tag, tip.pose, 9000);
        }

        dock() {
            this.state.isDocked = true;
            this.saveState();
            this.hideBubble();
            this.container.classList.add('docked');
        }

        undock() {
            this.state.isDocked = false;
            this.saveState();
            this.container.classList.remove('docked');
            this.pulse();
            this.speak("Qaytganingizdan xursandman! Yana biror fikr kerakmi? 🦀", 'Hamjamiyat', 'main', 5000);
        }
    }

    // Launch cleanly
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', () => {
            window.ferrisCompanion = new FerrisDevCompanion();
        });
    } else {
        window.ferrisCompanion = new FerrisDevCompanion();
    }
})();
