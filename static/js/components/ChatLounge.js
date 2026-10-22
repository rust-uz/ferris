import { toast } from './Toast.js';

export class ChatLounge {
    constructor(authManager) {
        this.auth = authManager;
        this.ws = null;
        this.activeRoom = 'general';
        this.messages = [];

        this.chatBox = document.getElementById('chatMessages');
        this.chatInput = document.getElementById('chatInput');
        this.sendBtn = document.getElementById('chatSendBtn');
        this.roomTabs = document.querySelectorAll('.room-tab');

        this.init();
    }

    init() {
        this.connectWs();

        if (this.sendBtn && this.chatInput) {
            this.sendBtn.addEventListener('click', () => this.sendMessage());
            this.chatInput.addEventListener('keypress', (e) => {
                if (e.key === 'Enter') this.sendMessage();
            });
        }

        this.roomTabs.forEach(tab => {
            tab.addEventListener('click', (e) => {
                const room = e.currentTarget.getAttribute('data-room') || 'general';
                this.switchRoom(room);
            });
        });

        this.auth.onAuthChange((user) => {
            this.updateInputState(user);
        });
    }

    updateInputState(user) {
        if (!this.chatInput || !this.sendBtn) return;
        if (user) {
            this.chatInput.disabled = false;
            this.chatInput.placeholder = `#${this.activeRoom} ga xabar yozish (@${user.username})...`;
            this.sendBtn.disabled = false;
        } else {
            this.chatInput.disabled = true;
            this.chatInput.placeholder = 'Xabar yozish uchun Telegram orqali kiring 🔒';
            this.sendBtn.disabled = true;
        }
    }

    switchRoom(room) {
        this.activeRoom = room;
        this.roomTabs.forEach(t => {
            if (t.getAttribute('data-room') === room) {
                t.classList.add('active');
            } else {
                t.classList.remove('active');
            }
        });
        this.renderMessages();
        this.updateInputState(this.auth.user);
    }

    connectWs() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws/community`;

        this.ws = new WebSocket(wsUrl);

        this.ws.onopen = () => {
            console.log('📡 [Ferris Live Lounge] WebSocket connected');
        };

        this.ws.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                this.messages.push(msg);
                this.renderMessages();
            } catch (err) {
                console.error('WS parse error:', err);
            }
        };

        this.ws.onclose = () => {
            console.warn('WS connection lost, reconnecting in 3s...');
            setTimeout(() => this.connectWs(), 3000);
        };
    }

    sendMessage() {
        if (!this.auth.user || !this.auth.token) {
            this.auth.openModal();
            toast.show('Xabar yozish uchun avval tizimga kiring!', 'info');
            return;
        }

        const text = this.chatInput.value.trim();
        if (!text) return;

        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            const payload = {
                token: this.auth.token,
                room: this.activeRoom,
                text: text
            };
            this.ws.send(JSON.stringify(payload));
            this.chatInput.value = '';
        } else {
            toast.show('Server bilan aloqa uzilgan', 'error');
        }
    }

    renderMessages() {
        if (!this.chatBox) return;

        const filtered = this.messages.filter(m => !m.room || m.room === this.activeRoom);
        
        if (filtered.length === 0) {
            this.chatBox.innerHTML = `
                <div class="chat-empty">
                    <span class="empty-icon">💬</span>
                    <p>Ushbu xonada hali xabarlar yo'q. Birinchi bo'lib yozing!</p>
                </div>
            `;
            return;
        }

        this.chatBox.innerHTML = filtered.map(m => {
            const time = new Date(m.timestamp * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            const avatar = m.user.avatar_url 
                ? `<img src="${m.user.avatar_url}" class="msg-avatar" alt="Avatar"/>` 
                : `<div class="msg-avatar-placeholder">${m.user.display_name.charAt(0)}</div>`;

            const badge = m.user.provider === 'telegram' ? '🔵 TG' : '🐙 GH';

            return `
                <div class="chat-msg-row">
                    ${avatar}
                    <div class="msg-bubble">
                        <div class="msg-meta">
                            <span class="msg-author">${m.user.display_name}</span>
                            <span class="user-badge">${badge}</span>
                            <span class="msg-time">${time}</span>
                        </div>
                        <div class="msg-content">${this.escapeHtml(m.text)}</div>
                    </div>
                </div>
            `;
        }).join('');

        this.chatBox.scrollTop = this.chatBox.scrollHeight;
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.innerText = text;
        return div.innerHTML;
    }
}
