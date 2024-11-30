//% IMPORTS

import autosuggest.CustomInputConnection;

//% END

//% QUAD_SURFACE_ON_CREATE_INPUT_CONNECTION

// Needed to fix error: unreachable statement in Java
if (true) {
    outAttrs.inputType = EditorInfo.TYPE_CLASS_TEXT
        | EditorInfo.TYPE_TEXT_FLAG_AUTO_CORRECT;
    outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN
        | EditorInfo.IME_ACTION_NONE;
    // fullEditor is false, but we might set this to true for enabling
    // text selection, and copy/paste. Lets see.
    return new CustomInputConnection(this, false);
}

//% END

