package app.farsail.android

import android.app.Activity
import android.view.View
import android.widget.LinearLayout

/** A non-modal reminder; remote viewing and system consent screens are never interrupted. */
class UpdateNoticeView(activity: Activity, action: () -> Unit) : LinearLayout(activity) {
    private val ui = MobileUi(activity)
    private val title: android.widget.TextView
    private val detail: android.widget.TextView
    private val button: android.widget.Button
    init {
        orientation = VERTICAL
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(4))
        background = ui.shape(android.graphics.Color.WHITE, 14, true)
        title = ui.text(this, "", 16f, bold = true)
        detail = ui.text(this, "", 13f, secondary = true)
        ui.gap(this, 10)
        button = ui.button(this, "", action = action)
        visibility = View.GONE
        layoutParams = LayoutParams(-1, -2).apply { bottomMargin = ui.dp(16) }
    }
    fun render(state: UpdateState) {
        visibility = if (state.phase in listOf("available", "downloading", "ready")) View.VISIBLE else View.GONE
        title.text = if (state.phase == "ready") "新版本 ${state.version} 已就绪" else "发现新版本 ${state.version}"
        detail.text = when (state.phase) {
            "ready" -> "在手机上确认安装即可，账号与设置会保留。"
            "downloading" -> "正在下载 ${state.progress}%，可继续使用。"
            else -> "可直接在手机上更新，无需连接电脑。"
        }
        button.text = when (state.phase) { "ready" -> "安装更新"; "downloading" -> "查看下载进度"; else -> "下载更新" }
    }
}
