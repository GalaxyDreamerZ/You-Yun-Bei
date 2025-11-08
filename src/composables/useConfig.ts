// 在 Tauri 环境下才按需加载日志插件，避免浏览器模式初始化错误
import {
    commands,
    DEFAULT_CONFIG,
    events,
    type Config,
} from '../bindings'
import { $t } from '../i18n'

// 定义默认配置
const defaultConfig: Config = DEFAULT_CONFIG as unknown as Config;
const { showError } = useNotification()
const config = ref(defaultConfig)
const isLoading = ref(false)

/**
 * 安全错误日志输出
 *
 * - 行为：在 Tauri 环境使用插件日志；在纯 Web 开发环境使用 console.error
 * - 目的：避免在浏览器模式下调用 Tauri 插件导致 `invoke` 未定义错误
 */
function logError(message: string) {
    const isTauriEnv = typeof window !== 'undefined'
        && (window as any).__TAURI__
        && (window as any).__TAURI__.core
        && typeof (window as any).__TAURI__.core.invoke === 'function'
    if (isTauriEnv) {
        // 动态导入 tauri 日志插件，避免在浏览器模式下触发初始化错误
        // eslint-disable-next-line @typescript-eslint/no-floating-promises
        import('@tauri-apps/plugin-log')
            .then((mod) => {
                try {
                    mod.error(message)
                } catch (e) {
                    // eslint-disable-next-line no-console
                    console.error('[tauri-log-fallback]', message, e)
                }
            })
            .catch((e) => {
                // eslint-disable-next-line no-console
                console.error('[tauri-log-import-failed]', message, e)
            })
    } else {
        // 退化为浏览器控制台输出
        // eslint-disable-next-line no-console
        console.error(message)
    }
}

async function refreshConfig() {
    isLoading.value = true
    try {
        const result = await commands.getLocalConfig()
        if (result.status === 'error') {
            throw new Error(result.error)
        }
        config.value = result.data
    } catch (e) {
        logError(`Failed to load config: ${e}`)
        showError({
            message: $t('error.config_load_failed')
        })
        // 加载失败时使用默认配置
        config.value = defaultConfig
    } finally {
        isLoading.value = false
    }
}

async function saveConfig() {
    try {
        const result = await commands.setConfig(config.value)
        if (result.status === 'error') {
            throw new Error(result.error)
        }
    } catch (e) {
        logError(`Failed to set config: ${e}`)
        showError({
            message: $t('error.set_config_failed')
        })
    }
}

if (import.meta.client) {
    const isTauriEnv = typeof window !== 'undefined'
        && (window as any).__TAURI__
        && (window as any).__TAURI__.core
        && typeof (window as any).__TAURI__.core.invoke === 'function'
    if (isTauriEnv) {
        events.quickActionCompleted
            .listen((event) => {
                const payload = event.payload
                if (
                    payload.status === 'Success' &&
                    payload.operation === 'Backup'
                ) {
                    void refreshConfig()
                }
            })
            .catch((err) => {
                logError(`Failed to listen quick action events: ${err}`)
            })
    }
}
// 初始加载：仅在 Tauri 环境下调用后端命令
if (typeof window !== 'undefined'
    && (window as any).__TAURI__
    && (window as any).__TAURI__.core
    && typeof (window as any).__TAURI__.core.invoke === 'function') {
    refreshConfig()
}

export function useConfig() {
    return {
        config,
        isLoading,
        refreshConfig,
        saveConfig
    }
}
