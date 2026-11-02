package app.farsail.android

object NativeBridge {
    init { System.loadLibrary("farsail_android") }
    external fun initialize(store: CredentialStore, context: android.content.Context): String
    external fun call(op: String, args: String): String
    external fun verifyUpdate(envelope: String, installed: Long): String
    external fun begin(): Long
    external fun stopLocal()
    external fun setControl(enabled: Boolean)
    external fun geometry(width: Int, height: Int): Long
    external fun publish(jpeg: ByteArray, width: Int, height: Int, layout: Long): Boolean
    external fun drain(): String
    external fun inputValid(session: String, epoch: Long, layout: Long): Boolean
}
