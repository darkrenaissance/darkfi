//% IMPORTS

import android.view.ViewGroup;
import android.view.WindowInsets.Type;
import java.util.HashMap;

import videodecode.VideoDecoder;

//% END

//% RESIZING_LAYOUT_BODY

native static void onApplyInsets(
    int sys_left, int sys_top, int sys_right, int sys_bottom,
    int ime_left, int ime_top, int ime_right, int ime_bottom
);

//% END

//% RESIZING_LAYOUT_ON_APPLY_WINDOW_INSETS

{
    Insets imeInsets = insets.getInsets(WindowInsets.Type.ime());
    Insets sysInsets = insets.getInsets(WindowInsets.Type.systemBars());

    // Screen: (1440, 3064)
    // IME height: 1056
    // Sys insets: (0, 152, 0, 135)

    onApplyInsets(
        sysInsets.left, sysInsets.top, sysInsets.right, sysInsets.bottom,
        imeInsets.left, imeInsets.top, imeInsets.right, imeInsets.bottom
    );
}
// Workaround for Java error due to remaining body.
// We handle the insets in our app directly.
if (true)
    return insets;

//% END

//% MAIN_ACTIVITY_BODY

public String getAppDataPath() {
    return getApplicationContext().getDataDir().getAbsolutePath();
}
public String getExternalStoragePath() {
    return getApplicationContext().getExternalFilesDir(null).getAbsolutePath();
}

public int getKeyboardHeight() {
    WindowInsets windowInsets = view.getRootWindowInsets();
    if (windowInsets == null) {
        return 0;
    }
    Insets imeInsets = windowInsets.getInsets(WindowInsets.Type.ime());
    Insets navInsets = windowInsets.getInsets(WindowInsets.Type.navigationBars());
    return Math.max(0, imeInsets.bottom - navInsets.bottom);
}

public float getScreenDensity() {
    return getResources().getDisplayMetrics().density;
}

public boolean isImeVisible() {
    View decorView = getWindow().getDecorView();
    WindowInsets insets = decorView.getRootWindowInsets();
    Insets imeInsets = insets.getInsets(WindowInsets.Type.ime());
    if (imeInsets == null)
        return false;
    return insets.isVisible(Type.ime());
}

public VideoDecoder createVideoDecoder() {
    VideoDecoder decoder = new VideoDecoder();
    decoder.setContext(this);
    return decoder;
}

//% END

//% MAIN_ACTIVITY_ON_CREATE

view.setFocusable(false);
view.setFocusableInTouchMode(false);
view.clearFocus();

// Start a foreground service so the app stays awake
Intent serviceIntent = new Intent(this, ForegroundService.class);
startForegroundService(serviceIntent);

//% END

