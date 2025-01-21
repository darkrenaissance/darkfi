//% IMPORTS

import android.view.inputmethod.InputMethodManager;
import android.os.Environment;

import autosuggest.CustomInputConnection;

//% END

//% MAIN_ACTIVITY_BODY

public void cancelComposition() {
    InputMethodManager imm =
        (InputMethodManager)getSystemService(Context.INPUT_METHOD_SERVICE);
    imm.restartInput(view);
}

public String getAppDataPath() {
    return getApplicationContext().getDataDir().getAbsolutePath();
}
public String getExternalStoragePath() {
    return getApplicationContext().getExternalFilesDir(null).getAbsolutePath();
    //return Environment.getExternalStorageDirectory().getAbsolutePath();
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

//% END

//% QUAD_SURFACE_ON_CREATE_INPUT_CONNECTION

// Needed to fix error: unreachable statement in Java
if (true) {
    outAttrs.inputType = EditorInfo.TYPE_CLASS_TEXT
        | EditorInfo.TYPE_TEXT_FLAG_AUTO_CORRECT;
    outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN
        | EditorInfo.IME_ACTION_NONE;
    return new CustomInputConnection(this, outAttrs);
}

//% END

