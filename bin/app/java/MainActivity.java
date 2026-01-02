//% IMPORTS

import android.view.ViewGroup;
import android.view.WindowInsets.Type;
import android.view.inputmethod.EditorInfo;
import android.text.InputType;
import java.util.HashMap;

import videodecode.VideoDecoder;
import textinput.InputConnection;
import textinput.Settings;
import textinput.Listener;
import textinput.State;
import textinput.GameTextInput;

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

// GameTextInput native bridge functions (following official Android pattern)
native void setInputConnectionNative(textinput.InputConnection c);
native void onTextInputEventNative(textinput.State softKeyboardEvent);

// GameTextInput InputConnection reference (public for QuadSurface access)
public textinput.InputConnection gameTextInputInputConnection;

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

//view.setFocusable(false);
//view.setFocusableInTouchMode(false);
//view.clearFocus();

// Start a foreground service so the app stays awake
Intent serviceIntent = new Intent(this, ForegroundService.class);
startForegroundService(serviceIntent);

//% END


//% QUAD_SURFACE_ON_CREATE_INPUT_CONNECTION

// Get reference to MainActivity
        if (getContext() == null)
            Log.i("darkfi", "getCTX (on creat) is nulllll!!!!!!!!!!!!!!!!!!");
MainActivity mainActivity = (MainActivity)getContext();

android.util.Log.d("darkfi", "onCreateInputConnection called");

// Create InputConnection if it doesn't exist yet
if (mainActivity.gameTextInputInputConnection == null) {
    android.util.Log.d("darkfi", "Creating new InputConnection");
    // Create InputConnection with Context (from QuadSurface)
    android.view.inputmethod.EditorInfo editorInfo = new android.view.inputmethod.EditorInfo();
    editorInfo.inputType = android.text.InputType.TYPE_CLASS_TEXT |
                           android.text.InputType.TYPE_TEXT_FLAG_AUTO_CORRECT;
    editorInfo.imeOptions = android.view.inputmethod.EditorInfo.IME_FLAG_NO_FULLSCREEN;

    if (mainActivity == null)
        Log.i("darkfi", "mainact is NULLLL");
    mainActivity.gameTextInputInputConnection = new textinput.InputConnection(
        getContext(),
        this,
        new textinput.Settings(editorInfo, true)
    );

    // Pass the InputConnection to native GameTextInput library
    android.util.Log.d("darkfi", "InputConnection created and passed to native");
    mainActivity.setInputConnectionNative(mainActivity.gameTextInputInputConnection);
} else {
    android.util.Log.d("darkfi", "Reusing existing InputConnection");
}

// Set the listener to receive IME state changes
mainActivity.gameTextInputInputConnection.setListener(new textinput.Listener() {
    @Override
    public void stateChanged(textinput.State newState, boolean dismissed) {
        // Called when the IME sends new text state
        // Forward to native code which triggers Rust callback
        android.util.Log.d("darkfi", "stateChanged: text=" + newState.toString());
        mainActivity.onTextInputEventNative(newState);
    }

    @Override
    public void onImeInsetsChanged(androidx.core.graphics.Insets insets) {
        // Called when IME insets change (e.g., keyboard height changes)
        // Optional: can be used for dynamic layout adjustment
    }

    @Override
    public void onSoftwareKeyboardVisibilityChanged(boolean visible) {
        // Called when keyboard is shown or hidden
        android.util.Log.d("darkfi", "onSoftwareKeyboardVisibilityChanged: " + visible);
    }

    @Override
    public void onEditorAction(int actionCode) {
        // Called when user presses action button (Done, Next, etc.)
        // Optional: handle specific editor actions
    }
});

// Copy EditorInfo from GameTextInput to configure IME
if (outAttrs != null) {
    textinput.GameTextInput.copyEditorInfo(
        mainActivity.gameTextInputInputConnection.getEditorInfo(),
        outAttrs
    );
}

// Return the GameTextInput InputConnection to IME
if (true) return mainActivity.gameTextInputInputConnection;
return mainActivity.gameTextInputInputConnection;

//% END
