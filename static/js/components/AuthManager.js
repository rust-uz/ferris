import { toast } from './Toast.js';

export class AuthManager {
    constructor() {
        this.token = localStorage.getItem('ferris_session') || null;
        this.user = null;
        this.onAuthChangeCallbacks = [];

        this.init();
    }

    onAuthChange(cb) {
        this.onAuthChangeCallbacks.push(cb);
    }

    notify() {
        this.onAuthChangeCallbacks.forEach(cb => cb(this.user, this.token));
    }

    async init() {
        window.onTelegramAuth = this.handleTelegramAuth.bind(this);

        // Check if redirected from GitHub with token
        const urlParams = new URLSearchParams(window.location.search);
        const ghToken = urlParams.get('github_token');
        if (ghToken) {
            this.token = ghToken;
            localStorage.setItem('ferris_session', this.token);
            window.history.replaceState({}, document.title, window.location.pathname);
            toast.show('GitHub orqali muvaffaqiyatli kirdingiz! 🐙', 'success');
        }

        if (this.token) {
            await this.fetchMe();
        } else {
            this.notify();
        }
    }

    async fetchMe() {
        try {
            const res = await fetch(`/api/auth/me?token=${this.token}`);
            if (res.ok) {
                this.user = await res.json();
                this.notify();
            } else {
                this.logout();
            }
        } catch (e) {
            console.error('Fetch me error:', e);
            this.notify();
        }
    }

    async handleTelegramAuth(user) {
        console.log('Telegram auth callback received:', user);
        try {
            toast.show('Telegram imzosi tekshirilmoqda...', 'info');
            const res = await fetch('/api/auth/telegram', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(user)
            });

            const data = await res.json();
            if (res.ok && data.success) {
                this.token = data.token;
                this.user = data.user;
                localStorage.setItem('ferris_session', this.token);
                toast.show(`Xush kelibsiz, @${this.user.username}!`, 'success');
                this.notify();
                this.closeModal();
            } else {
                toast.show(data.error || 'Autentifikatsiya xatosi', 'error');
            }
        } catch (err) {
            console.error('Telegram auth request failed:', err);
            toast.show('Server bilan bog\'lanishda xatolik', 'error');
        }
    }

    async logout() {
        if (this.token) {
            try {
                await fetch(`/api/auth/logout?token=${this.token}`, { method: 'POST' });
            } catch (e) {}
        }
        localStorage.removeItem('ferris_session');
        this.token = null;
        this.user = null;
        toast.show('Hisobdan chiqildi', 'info');
        this.notify();
    }

    openModal() {
        const modal = document.getElementById('authModal');
        if (modal) modal.classList.remove('hidden');
    }

    closeModal() {
        const modal = document.getElementById('authModal');
        if (modal) modal.classList.add('hidden');
    }
}
