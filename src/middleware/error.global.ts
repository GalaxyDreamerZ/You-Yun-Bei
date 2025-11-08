import { warn } from "@tauri-apps/plugin-log"

const { config } = useConfig()
// 保持与当前项目路由大小写一致，并允许这些页面的子路径（防止子页面被误重定向）
const allowedPrefixes = ["/About", "/AddGame", "/Settings", "/SyncSettings", "/Management"]

export default defineNuxtRouteMiddleware((to, from) => {
    /**
     * 函数级注释：管理页路由校验
     * 仅当访问 Management 下且带有 name 参数时，校验该游戏是否存在；不存在则跳转首页。
     */
    if (to.fullPath.startsWith("/Management") && (to.params as any)?.name) {
        const name = (to.params as any).name as string
        const found = config.value.games.find((x) => x.name === name)
        if (!found) {
            const isTauriEnv = typeof window !== 'undefined' && (window as any).__TAURI__ && (window as any).__TAURI__.invoke
            if (isTauriEnv) {
                warn(`Game ${name} not found`)
            } else {
                // eslint-disable-next-line no-console
                console.warn(`Game ${name} not found`)
            }
            return navigateTo("/")
        }
    }

    /**
     * 函数级注释：一般页面白名单校验（支持子路径）
     * 允许以 allowedPrefixes 开头的路径通过（例如 /AddGame/xxx、/Settings/xxx）。
     * 对不在白名单且非 Management 的路径，记录警告并重定向首页。
     */
    // 首页仅允许精确匹配 "/"，其他页面允许子路径前缀匹配
    const isRoot = to.fullPath === "/"
    const isAllowed = isRoot || allowedPrefixes.some(prefix => to.fullPath === prefix || to.fullPath.startsWith(prefix))
    if (!isAllowed) {
        const isTauriEnv = typeof window !== 'undefined' && (window as any).__TAURI__ && (window as any).__TAURI__.invoke
        if (isTauriEnv) {
            warn(`Page ${to.fullPath} not found`)
        } else {
            // eslint-disable-next-line no-console
            console.warn(`Page ${to.fullPath} not found`)
        }
        return navigateTo("/")
    }
})